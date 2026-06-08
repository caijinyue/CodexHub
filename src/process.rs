use crate::profile;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus {
    pub plan_type: Option<String>,
    pub primary_remaining_percent: Option<u8>,
    pub secondary_remaining_percent: Option<u8>,
    pub primary_resets_at: Option<chrono::DateTime<chrono::Local>>,
    pub secondary_resets_at: Option<chrono::DateTime<chrono::Local>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySession {
    pub profile: String,
    pub codex_home: PathBuf,
    pub is_codexhub_profile: bool,
    pub session_id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub path: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewMessage {
    pub role: PreviewRole,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMethod {
    DeviceCode,
    Web,
}

impl LoginMethod {
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::DeviceCode => &["login", "--device-auth"],
            Self::Web => &["login"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DeviceCode => "device code",
            Self::Web => "web login",
        }
    }
}

pub fn codex_login(name: &str, method: LoginMethod) -> Result<i32> {
    run_codex(name, method.args().iter().copied())
}

pub fn codex_run(name: &str, args: &[String]) -> Result<i32> {
    run_codex(name, args.iter().map(String::as_str))
}

pub fn codex_exec(name: &str, args: &[String]) -> Result<i32> {
    let mut all = vec!["exec".to_string()];
    all.extend(args.iter().cloned());
    run_codex(name, all.iter().map(String::as_str))
}

pub fn codex_resume(name: &str, session_id: &str) -> Result<i32> {
    run_codex(name, ["resume", "--all", session_id])
}

pub fn codex_resume_session(session: &HistorySession) -> Result<i32> {
    run_codex_home(
        &session.codex_home,
        ["resume", "--all", &session.session_id],
    )
}

pub fn codex_resume_copied_session(session: &HistorySession, target_profile: &str) -> Result<i32> {
    copy_session_to_profile(session, target_profile)?;
    codex_resume(target_profile, &session.session_id)
}

pub fn copy_session_to_profile(session: &HistorySession, target_profile: &str) -> Result<()> {
    if session.is_codexhub_profile && session.profile == target_profile {
        return Ok(());
    }

    let path = session
        .path
        .as_deref()
        .context("Selected session does not expose a rollout path")?;
    if session.is_codexhub_profile {
        profile::copy_session_to_profile(
            &session.profile,
            target_profile,
            &session.session_id,
            path,
        )?;
    } else {
        profile::copy_session_root_to_profile(
            &session.codex_home,
            target_profile,
            &session.session_id,
            path,
        )?;
    }
    Ok(())
}

pub fn session_preview_messages(session: &HistorySession, max_lines: usize) -> Vec<PreviewMessage> {
    let Some(path) = session.path.as_deref() else {
        return vec![PreviewMessage::system("No rollout file path available.")];
    };
    let Ok(file) = File::open(path) else {
        return vec![PreviewMessage::system(format!(
            "Cannot open rollout file: {path}"
        ))];
    };
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if let Some(message) = preview_line(&line) {
            push_preview_message(&mut messages, message);
        }
    }
    let mut out = Vec::new();
    for (idx, message) in messages.into_iter().rev().enumerate() {
        if idx > 0 {
            out.push(PreviewMessage::system(""));
        }
        push_wrapped_message_preview(&mut out, message.role, &message.text, 120);
        if out.len() >= max_lines {
            out.truncate(max_lines);
            break;
        }
    }
    if out.is_empty() {
        vec![PreviewMessage::system(
            "No user or assistant messages found in rollout.",
        )]
    } else {
        out
    }
}

impl PreviewMessage {
    fn system(text: impl Into<String>) -> Self {
        Self {
            role: PreviewRole::Assistant,
            text: text.into(),
        }
    }
}

pub fn run_codex<'a, I>(name: &str, args: I) -> Result<i32>
where
    I: IntoIterator<Item = &'a str>,
{
    let home = profile::ensure_exists(name)?;
    run_codex_home(&home, args)
}

fn run_codex_home<'a, I>(home: &std::path::Path, args: I) -> Result<i32>
where
    I: IntoIterator<Item = &'a str>,
{
    let status = Command::new("codex")
        .args(args)
        .env("CODEX_HOME", home)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| "Failed to execute official codex CLI")?;
    Ok(status.code().unwrap_or(1))
}

pub fn codex_account_status(name: &str) -> Result<AccountStatus> {
    let text = app_server_request(name, 2, r#"{"id":2,"method":"account/rateLimits/read"}"#)?;
    parse_account_status(&text).context("Parsing codex account status")
}

pub fn codex_history_sessions(name: &str, limit: usize) -> Result<Vec<HistorySession>> {
    let home = profile::ensure_exists(name)?;
    codex_history_sessions_from_home(name, home, true, limit)
}

pub fn codex_history_sessions_from_home(
    label: &str,
    home: PathBuf,
    is_codexhub_profile: bool,
    limit: usize,
) -> Result<Vec<HistorySession>> {
    let app_server_sessions = app_server_request_home(
        &home,
        3,
        &format!(
            r#"{{"id":3,"method":"thread/list","params":{{"limit":{limit},"sortKey":"updated_at","sortDirection":"desc","sourceKinds":[],"archived":false,"cwd":null,"useStateDbOnly":false}}}}"#
        ),
    )
    .ok()
    .and_then(|text| parse_history_sessions(label, home.clone(), is_codexhub_profile, &text));
    let mut sessions = app_server_sessions
        .unwrap_or_else(|| local_history_sessions(label, home, is_codexhub_profile, limit));
    sessions.retain(|session| {
        session
            .path
            .as_deref()
            .map(|path| Path::new(path).is_file())
            .unwrap_or(true)
    });
    sessions.truncate(limit);
    Ok(sessions)
}

pub fn delete_history_session(session: &HistorySession) -> Result<()> {
    let Some(path) = session.path.as_deref() else {
        anyhow::bail!("Selected session does not expose a rollout path");
    };
    let path = PathBuf::from(path);
    let relative = path.strip_prefix(&session.codex_home).with_context(|| {
        format!(
            "Session file {} is not under CODEX_HOME {}",
            path.display(),
            session.codex_home.display()
        )
    })?;
    if relative
        .components()
        .next()
        .and_then(|component| match component {
            std::path::Component::Normal(value) => Some(value),
            _ => None,
        })
        != Some(std::ffi::OsStr::new("sessions"))
    {
        anyhow::bail!("Refusing to delete non-session file {}", path.display());
    }
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("Deleting {}", path.display()))?;
    }
    remove_session_index_line(
        &session.codex_home.join("session_index.jsonl"),
        &session.session_id,
    )?;
    remove_session_index_line(
        &session.codex_home.join("history.jsonl"),
        &session.session_id,
    )?;
    Ok(())
}

fn app_server_request(name: &str, request_id: u64, request: &str) -> Result<String> {
    let home = profile::ensure_exists(name)?;
    app_server_request_home(&home, request_id, request)
}

fn app_server_request_home(
    home: &std::path::Path,
    request_id: u64,
    request: &str,
) -> Result<String> {
    let mut child = Command::new("codex")
        .args(["app-server", "--listen", "stdio://"])
        .env("CODEX_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| "Failed to start official codex app-server")?;

    let mut stdin = child.stdin.take().context("Opening app-server stdin")?;
    let stdout = child.stdout.take().context("Opening app-server stdout")?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    writeln!(
        stdin,
        r#"{{"id":1,"method":"initialize","params":{{"clientInfo":{{"name":"codexhub","title":"CodexHub","version":"{}"}},"capabilities":{{"experimentalApi":true,"requestAttestation":false}}}}}}"#,
        env!("CARGO_PKG_VERSION")
    )?;
    let mut out = wait_for_app_server_response(&rx, 1, Duration::from_secs(6))?;
    writeln!(stdin, "{request}")?;
    let timeout = if request_id == 2 {
        Duration::from_secs(8)
    } else {
        Duration::from_secs(6)
    };
    out.push_str(&wait_for_app_server_response(&rx, request_id, timeout)?);
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    Ok(out)
}

fn wait_for_app_server_response(
    rx: &mpsc::Receiver<String>,
    request_id: u64,
    timeout: Duration,
) -> Result<String> {
    let mut out = String::new();
    loop {
        let line = rx
            .recv_timeout(timeout)
            .with_context(|| format!("Timed out waiting for app-server response {request_id}"))?;
        out.push_str(&line);
        out.push('\n');
        if response_has_id(&line, request_id) {
            return Ok(out);
        }
    }
}

fn response_has_id(line: &str, request_id: u64) -> bool {
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|value| value.pointer("/id").and_then(Value::as_u64))
        == Some(request_id)
}

fn parse_account_status(text: &str) -> Option<AccountStatus> {
    for line in text.lines() {
        let value: AppServerLine = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.id == Some(2) {
            let limits = value.result?.rate_limits;
            return Some(AccountStatus {
                plan_type: limits.plan_type,
                primary_remaining_percent: limits
                    .primary
                    .as_ref()
                    .and_then(|w| remaining_percent(w.used_percent)),
                secondary_remaining_percent: limits
                    .secondary
                    .as_ref()
                    .and_then(|w| remaining_percent(w.used_percent)),
                primary_resets_at: limits
                    .primary
                    .as_ref()
                    .and_then(|w| reset_time(w.resets_at)),
                secondary_resets_at: limits
                    .secondary
                    .as_ref()
                    .and_then(|w| reset_time(w.resets_at)),
            });
        }
    }
    None
}

fn remaining_percent(used_percent: u8) -> Option<u8> {
    Some(100u8.saturating_sub(used_percent.min(100)))
}

fn reset_time(timestamp: Option<i64>) -> Option<chrono::DateTime<chrono::Local>> {
    timestamp
        .and_then(|timestamp| chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0))
        .map(chrono::DateTime::<chrono::Local>::from)
}

fn parse_history_sessions(
    profile: &str,
    codex_home: PathBuf,
    is_codexhub_profile: bool,
    text: &str,
) -> Option<Vec<HistorySession>> {
    for line in text.lines() {
        let value: ThreadListLine = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.id == Some(3) {
            let mut sessions: Vec<_> = value
                .result?
                .data
                .into_iter()
                .map(|thread| HistorySession {
                    profile: profile.to_string(),
                    codex_home: codex_home.clone(),
                    is_codexhub_profile,
                    session_id: thread.session_id,
                    title: thread
                        .name
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| trim_preview(&thread.preview)),
                    cwd: thread.cwd,
                    path: thread.path,
                    updated_at: thread.updated_at,
                })
                .collect();
            sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
            return Some(sessions);
        }
    }
    None
}

fn local_history_sessions(
    label: &str,
    home: PathBuf,
    is_codexhub_profile: bool,
    limit: usize,
) -> Vec<HistorySession> {
    let index = home.join("session_index.jsonl");
    let Ok(data) = fs::read_to_string(&index) else {
        return Vec::new();
    };
    let mut sessions: Vec<_> = data
        .lines()
        .filter_map(|line| local_history_session(label, &home, is_codexhub_profile, line))
        .collect();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions.truncate(limit);
    sessions
}

fn local_history_session(
    label: &str,
    home: &Path,
    is_codexhub_profile: bool,
    line: &str,
) -> Option<HistorySession> {
    let value: LocalSessionIndexLine = serde_json::from_str(line).ok()?;
    let session_id = value.id?;
    let rollout_path = find_session_rollout(home, &session_id);
    let cwd = rollout_path.as_deref().and_then(session_meta_cwd);
    let path = rollout_path.map(|path| path.display().to_string());
    let updated_at = value
        .updated_at
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.timestamp())
        .unwrap_or(0);
    Some(HistorySession {
        profile: label.to_string(),
        codex_home: home.to_path_buf(),
        is_codexhub_profile,
        session_id,
        title: value
            .thread_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "(untitled)".into()),
        cwd,
        path,
        updated_at,
    })
}

fn find_session_rollout(home: &Path, session_id: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(home.join("sessions"))
        .into_iter()
        .filter_map(Result::ok)
        .find_map(|entry| {
            let path = entry.path();
            path.is_file()
                .then(|| {
                    path.file_name()?
                        .to_str()?
                        .contains(session_id)
                        .then(|| path.to_path_buf())
                })
                .flatten()
        })
}

fn session_meta_cwd(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut lines = BufReader::new(file).lines();
    let line = lines.next()?.ok()?;
    let value: Value = serde_json::from_str(&line).ok()?;
    if value.pointer("/type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    value
        .pointer("/payload/cwd")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn remove_session_index_line(path: &Path, session_id: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let data = fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
    let kept: Vec<_> = data
        .lines()
        .filter(|line| !line.contains(session_id))
        .collect();
    let mut output = kept.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    fs::write(path, output).with_context(|| format!("Writing {}", path.display()))?;
    Ok(())
}

fn trim_preview(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        "(untitled)".into()
    } else if collapsed.chars().count() > 80 {
        format!("{}...", collapsed.chars().take(77).collect::<String>())
    } else {
        collapsed
    }
}

fn preview_line(line: &str) -> Option<PreviewMessage> {
    let value: Value = serde_json::from_str(line).ok()?;
    match value.pointer("/type").and_then(Value::as_str) {
        Some("event_msg") => event_message_preview(&value),
        Some("response_item") => response_item_preview(&value),
        _ => None,
    }
}

fn event_message_preview(value: &Value) -> Option<PreviewMessage> {
    let role = match value.pointer("/payload/type").and_then(Value::as_str)? {
        "user_message" => PreviewRole::User,
        "agent_message" => PreviewRole::Assistant,
        _ => return None,
    };
    let message = value
        .pointer("/payload/message")
        .and_then(Value::as_str)
        .map(clean_preview_text)?;
    if message.is_empty() {
        None
    } else {
        Some(PreviewMessage {
            role,
            text: message,
        })
    }
}

fn response_item_preview(value: &Value) -> Option<PreviewMessage> {
    let payload = value.pointer("/payload")?;
    if payload.pointer("/type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let role = payload.pointer("/role").and_then(Value::as_str)?;
    let role = match role {
        "user" => PreviewRole::User,
        "assistant" => PreviewRole::Assistant,
        _ => return None,
    };
    let text = message_content_text(payload.pointer("/content")?)?;
    if text.is_empty() {
        return None;
    }
    Some(PreviewMessage { role, text })
}

fn message_content_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => Some(clean_preview_text(text)),
        Value::Array(items) => {
            let parts: Vec<_> = items
                .iter()
                .filter_map(|item| {
                    item.pointer("/text")
                        .or_else(|| item.pointer("/content"))
                        .and_then(Value::as_str)
                        .map(clean_preview_text)
                        .filter(|text| !text.is_empty())
                })
                .collect();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        _ => None,
    }
}

fn clean_preview_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn push_wrapped_preview(out: &mut Vec<String>, text: &str, width: usize) {
    if text.chars().count() <= width {
        out.push(text.to_string());
        return;
    }
    let mut current = String::new();
    for word in text.split_whitespace() {
        let next_len = current.chars().count() + usize::from(!current.is_empty()) + word.len();
        if next_len > width && !current.is_empty() {
            out.push(current);
            current = String::new();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        out.push(current);
    }
}

fn push_wrapped_message_preview(
    out: &mut Vec<PreviewMessage>,
    role: PreviewRole,
    text: &str,
    width: usize,
) {
    let mut wrapped = Vec::new();
    push_wrapped_preview(&mut wrapped, text, width);
    out.extend(
        wrapped
            .into_iter()
            .map(|text| PreviewMessage { role, text }),
    );
}

fn push_preview_message(messages: &mut Vec<PreviewMessage>, message: PreviewMessage) {
    if messages
        .last()
        .is_some_and(|last| last.role == message.role && last.text == message.text)
    {
        return;
    }
    messages.push(message);
}

#[derive(Debug, Deserialize)]
struct AppServerLine {
    id: Option<u64>,
    result: Option<RateLimitsResponse>,
}

#[derive(Debug, Deserialize)]
struct RateLimitsResponse {
    #[serde(rename = "rateLimits")]
    rate_limits: RateLimitSnapshot,
}

#[derive(Debug, Deserialize)]
struct RateLimitSnapshot {
    #[serde(rename = "planType")]
    plan_type: Option<String>,
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
}

#[derive(Debug, Deserialize)]
struct RateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: u8,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ThreadListLine {
    id: Option<u64>,
    result: Option<ThreadListResponse>,
}

#[derive(Debug, Deserialize)]
struct ThreadListResponse {
    data: Vec<ThreadSummary>,
}

#[derive(Debug, Deserialize)]
struct ThreadSummary {
    #[serde(rename = "sessionId")]
    session_id: String,
    preview: String,
    #[serde(rename = "updatedAt")]
    updated_at: i64,
    cwd: Option<String>,
    path: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocalSessionIndexLine {
    id: Option<String>,
    thread_name: Option<String>,
    updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_account_rate_limit_status() {
        let output = r#"{"id":1,"result":{"codexHome":"/tmp"}}
{"method":"remoteControl/status/changed","params":{"status":"disabled"}}
{"id":2,"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":1,"windowDurationMins":300,"resetsAt":1779973549},"secondary":{"usedPercent":19,"windowDurationMins":10080,"resetsAt":1780484441},"planType":"plus"}}}"#;

        let status = parse_account_status(output).unwrap();

        assert_eq!(status.plan_type.as_deref(), Some("plus"));
        assert_eq!(status.primary_remaining_percent, Some(99));
        assert_eq!(status.secondary_remaining_percent, Some(81));
        assert_eq!(
            status.primary_resets_at.map(|value| value.timestamp()),
            Some(1779973549)
        );
        assert_eq!(
            status.secondary_resets_at.map(|value| value.timestamp()),
            Some(1780484441)
        );
    }

    #[test]
    fn parses_resume_thread_list() {
        let output = r#"{"id":1,"result":{"codexHome":"/tmp"}}
{"method":"remoteControl/status/changed","params":{"status":"disabled"}}
{"id":3,"result":{"data":[{"id":"t1","sessionId":"s1","preview":"first user request with a long enough preview","updatedAt":200,"cwd":"/repo","path":"/tmp/source/sessions/rollout-s1.jsonl","name":null},{"id":"t2","sessionId":"s2","preview":"ignored","updatedAt":300,"cwd":"/repo2","path":"/tmp/source/sessions/rollout-s2.jsonl","name":"Named thread"}],"nextCursor":null,"backwardsCursor":null}}"#;

        let home = PathBuf::from("/tmp/source");
        let sessions = parse_history_sessions("work", home.clone(), true, output).unwrap();

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, "s2");
        assert_eq!(sessions[0].title, "Named thread");
        assert_eq!(sessions[1].profile, "work");
        assert_eq!(sessions[1].codex_home, home);
        assert!(sessions[1].is_codexhub_profile);
        assert_eq!(sessions[1].cwd.as_deref(), Some("/repo"));
        assert_eq!(
            sessions[1].path.as_deref(),
            Some("/tmp/source/sessions/rollout-s1.jsonl")
        );
    }

    #[test]
    fn extracts_rollout_preview_from_user_and_assistant_messages() {
        let user = r#"{"type":"event_msg","payload":{"type":"user_message","message":"hello from user","images":[]}}"#;
        let agent = r#"{"type":"event_msg","payload":{"type":"agent_message","message":"hello from agent","phase":"commentary"}}"#;
        let user_response_item = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello from response item user"}]}}"#;
        let assistant = r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hello from assistant"}]}}"#;
        let developer = r#"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"hidden"}]}}"#;

        assert_eq!(
            preview_line(user),
            Some(PreviewMessage {
                role: PreviewRole::User,
                text: "hello from user".into()
            })
        );
        assert_eq!(
            preview_line(user_response_item),
            Some(PreviewMessage {
                role: PreviewRole::User,
                text: "hello from response item user".into()
            })
        );
        assert_eq!(
            preview_line(assistant),
            Some(PreviewMessage {
                role: PreviewRole::Assistant,
                text: "hello from assistant".into()
            })
        );
        assert_eq!(
            preview_line(agent),
            Some(PreviewMessage {
                role: PreviewRole::Assistant,
                text: "hello from agent".into()
            })
        );
        assert_eq!(preview_line(developer), None);
    }

    #[test]
    fn renders_conversation_newest_first_with_spacing() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-preview-test-{stamp}"));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("rollout.jsonl");
        std::fs::write(
            &path,
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"old prompt","images":[]}}"#.to_string()
                + "\n"
                + r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"ignored assistant"}]}}"#
                + "\n"
                + r#"{"type":"event_msg","payload":{"type":"user_message","message":"new prompt","images":[]}}"#
                + "\n",
        )
        .unwrap();
        let session = HistorySession {
            profile: "work".into(),
            codex_home: root.clone(),
            session_id: "s1".into(),
            title: "test".into(),
            updated_at: 1,
            cwd: None,
            path: Some(path.display().to_string()),
            is_codexhub_profile: true,
        };

        assert_eq!(
            session_preview_messages(&session, 10),
            vec![
                PreviewMessage {
                    role: PreviewRole::User,
                    text: "new prompt".into()
                },
                PreviewMessage::system(""),
                PreviewMessage {
                    role: PreviewRole::Assistant,
                    text: "ignored assistant".into()
                },
                PreviewMessage::system(""),
                PreviewMessage {
                    role: PreviewRole::User,
                    text: "old prompt".into()
                },
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deduplicates_mirrored_response_and_event_messages() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-preview-dedupe-test-{stamp}"));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("rollout.jsonl");
        std::fs::write(
            &path,
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"same user"}]}}"#.to_string()
                + "\n"
                + r#"{"type":"event_msg","payload":{"type":"user_message","message":"same user","images":[]}}"#
                + "\n"
                + r#"{"type":"event_msg","payload":{"type":"agent_message","message":"same assistant","phase":"commentary"}}"#
                + "\n"
                + r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"same assistant"}]}}"#
                + "\n",
        )
        .unwrap();
        let session = HistorySession {
            profile: "work".into(),
            codex_home: root.clone(),
            session_id: "s1".into(),
            title: "test".into(),
            updated_at: 1,
            cwd: None,
            path: Some(path.display().to_string()),
            is_codexhub_profile: true,
        };

        assert_eq!(
            session_preview_messages(&session, 10),
            vec![
                PreviewMessage {
                    role: PreviewRole::Assistant,
                    text: "same assistant".into()
                },
                PreviewMessage::system(""),
                PreviewMessage {
                    role: PreviewRole::User,
                    text: "same user".into()
                },
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deletes_selected_history_session_file_and_indexes() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-delete-session-test-{stamp}"));
        let sessions = root.join("sessions").join("2026").join("06").join("04");
        std::fs::create_dir_all(&sessions).unwrap();
        let deleted = sessions.join("rollout-2026-06-04T00-00-00-delete-me.jsonl");
        let kept = sessions.join("rollout-2026-06-04T00-00-01-keep-me.jsonl");
        std::fs::write(&deleted, "{}\n").unwrap();
        std::fs::write(&kept, "{}\n").unwrap();
        std::fs::write(
            root.join("session_index.jsonl"),
            r#"{"id":"delete-me","thread_name":"Delete","updated_at":"2026-06-04T00:00:00Z"}"#
                .to_string()
                + "\n"
                + r#"{"id":"keep-me","thread_name":"Keep","updated_at":"2026-06-04T00:00:01Z"}"#
                + "\n",
        )
        .unwrap();
        std::fs::write(
            root.join("history.jsonl"),
            r#"{"session_id":"delete-me","ts":1,"text":"delete"}"#.to_string()
                + "\n"
                + r#"{"session_id":"keep-me","ts":2,"text":"keep"}"#
                + "\n",
        )
        .unwrap();
        let session = HistorySession {
            profile: "work".into(),
            codex_home: root.clone(),
            session_id: "delete-me".into(),
            title: "Delete".into(),
            updated_at: 1,
            cwd: None,
            path: Some(deleted.display().to_string()),
            is_codexhub_profile: true,
        };

        delete_history_session(&session).unwrap();

        assert!(!deleted.exists());
        assert!(kept.exists());
        assert!(!std::fs::read_to_string(root.join("session_index.jsonl"))
            .unwrap()
            .contains("delete-me"));
        assert!(std::fs::read_to_string(root.join("session_index.jsonl"))
            .unwrap()
            .contains("keep-me"));
        assert!(!std::fs::read_to_string(root.join("history.jsonl"))
            .unwrap()
            .contains("delete-me"));
        assert!(std::fs::read_to_string(root.join("history.jsonl"))
            .unwrap()
            .contains("keep-me"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_local_history_index_with_rollout_cwd() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-local-history-test-{stamp}"));
        let sessions = root.join("sessions").join("2026").join("06").join("04");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("rollout-2026-06-04T00-00-00-local-session.jsonl"),
            r#"{"type":"session_meta","payload":{"id":"local-session","cwd":"/repo"}}"#.to_string()
                + "\n",
        )
        .unwrap();
        std::fs::write(
            root.join("session_index.jsonl"),
            r#"{"id":"local-session","thread_name":"Local","updated_at":"2026-06-04T00:00:00Z"}"#,
        )
        .unwrap();

        let sessions = local_history_sessions("work", root.clone(), true, 10);

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "local-session");
        assert_eq!(sessions[0].title, "Local");
        assert_eq!(sessions[0].cwd.as_deref(), Some("/repo"));
        assert_eq!(sessions[0].updated_at, 1780531200);
        assert!(sessions[0]
            .path
            .as_deref()
            .unwrap()
            .contains("local-session"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn maps_login_methods_to_codex_args() {
        assert_eq!(LoginMethod::DeviceCode.args(), &["login", "--device-auth"]);
        assert_eq!(LoginMethod::Web.args(), &["login"]);
    }
}
