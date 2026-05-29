use crate::profile;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus {
    pub plan_type: Option<String>,
    pub primary_remaining_percent: Option<u8>,
    pub secondary_remaining_percent: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySession {
    pub profile: String,
    pub session_id: String,
    pub title: String,
    pub cwd: Option<String>,
    pub path: Option<String>,
    pub updated_at: i64,
}

pub fn codex_login(name: &str) -> Result<i32> {
    run_codex(name, ["login"])
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

pub fn codex_resume_copied_session(session: &HistorySession, target_profile: &str) -> Result<i32> {
    if session.profile == target_profile {
        return codex_resume(target_profile, &session.session_id);
    }

    let path = session
        .path
        .as_deref()
        .context("Selected session does not expose a rollout path")?;
    profile::copy_session_to_profile(&session.profile, target_profile, &session.session_id, path)?;
    codex_resume(target_profile, &session.session_id)
}

pub fn run_codex<'a, I>(name: &str, args: I) -> Result<i32>
where
    I: IntoIterator<Item = &'a str>,
{
    let home = profile::ensure_exists(name)?;
    let status = Command::new("codex")
        .args(args)
        .env("CODEX_HOME", &home)
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
    let text = app_server_request(
        name,
        3,
        &format!(
            r#"{{"id":3,"method":"thread/list","params":{{"limit":{limit},"sortKey":"updated_at","sortDirection":"desc","sourceKinds":[],"archived":false,"cwd":null,"useStateDbOnly":false}}}}"#
        ),
    )?;
    parse_history_sessions(name, &text).context("Parsing codex resume session list")
}

fn app_server_request(name: &str, request_id: u64, request: &str) -> Result<String> {
    let home = profile::ensure_exists(name)?;
    let mut child = Command::new("codex")
        .args(["app-server", "--listen", "stdio://"])
        .env("CODEX_HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| "Failed to start official codex app-server")?;

    let mut stdin = child.stdin.take().context("Opening app-server stdin")?;
    writeln!(
        stdin,
        r#"{{"id":1,"method":"initialize","params":{{"clientInfo":{{"name":"codexhub","title":"CodexHub","version":"{}"}},"capabilities":{{"experimentalApi":true,"requestAttestation":false}}}}}}"#,
        env!("CARGO_PKG_VERSION")
    )?;
    thread::sleep(Duration::from_millis(500));
    writeln!(stdin, "{request}")?;
    thread::sleep(Duration::from_secs(if request_id == 2 { 4 } else { 3 }));
    drop(stdin);
    let _ = child.kill();

    let out = child
        .wait_with_output()
        .with_context(|| "Reading codex app-server output")?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
                    .and_then(|w| remaining_percent(w.used_percent)),
                secondary_remaining_percent: limits
                    .secondary
                    .and_then(|w| remaining_percent(w.used_percent)),
            });
        }
    }
    None
}

fn remaining_percent(used_percent: u8) -> Option<u8> {
    Some(100u8.saturating_sub(used_percent.min(100)))
}

fn parse_history_sessions(profile: &str, text: &str) -> Option<Vec<HistorySession>> {
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
    }

    #[test]
    fn parses_resume_thread_list() {
        let output = r#"{"id":1,"result":{"codexHome":"/tmp"}}
{"method":"remoteControl/status/changed","params":{"status":"disabled"}}
{"id":3,"result":{"data":[{"id":"t1","sessionId":"s1","preview":"first user request with a long enough preview","updatedAt":200,"cwd":"/repo","path":"/tmp/source/sessions/rollout-s1.jsonl","name":null},{"id":"t2","sessionId":"s2","preview":"ignored","updatedAt":300,"cwd":"/repo2","path":"/tmp/source/sessions/rollout-s2.jsonl","name":"Named thread"}],"nextCursor":null,"backwardsCursor":null}}"#;

        let sessions = parse_history_sessions("work", output).unwrap();

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, "s2");
        assert_eq!(sessions[0].title, "Named thread");
        assert_eq!(sessions[1].profile, "work");
        assert_eq!(sessions[1].cwd.as_deref(), Some("/repo"));
        assert_eq!(
            sessions[1].path.as_deref(),
            Some("/tmp/source/sessions/rollout-s1.jsonl")
        );
    }
}
