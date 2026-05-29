use crate::config;
use crate::size;
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProfileInfo {
    pub name: String,
    pub path: PathBuf,
    pub logged_in: bool,
    pub auth_mtime: Option<DateTime<Local>>,
    pub plan_type: Option<String>,
    pub limit_5h_remaining: Option<u8>,
    pub limit_7day_remaining: Option<u8>,
    pub plan_expires_at: Option<DateTime<Local>>,
    pub sessions_size: u64,
    pub logs_size: u64,
    pub total_size: u64,
    pub shared_cache: bool,
}

impl ProfileInfo {
    pub fn apply_account_status(&mut self, status: crate::process::AccountStatus) {
        if status.plan_type.is_some() {
            self.plan_type = status.plan_type;
        }
        self.limit_5h_remaining = status.primary_remaining_percent;
        self.limit_7day_remaining = status.secondary_remaining_percent;
    }
}

pub fn profile_path(name: &str) -> Result<PathBuf> {
    config::ensure_profile_name(name)?;
    Ok(config::paths()?.profiles.join(name))
}

pub fn create(name: &str, copy_config: bool) -> Result<PathBuf> {
    let paths = config::init()?;
    config::ensure_profile_name(name)?;
    let path = paths.profiles.join(name);
    if path.exists() {
        anyhow::bail!("Profile already exists: {name}");
    }
    fs::create_dir_all(path.join("sessions"))
        .with_context(|| format!("Creating {}", path.display()))?;
    if copy_config {
        if let Some(src) = config::default_codex_config() {
            if src.exists() {
                fs::copy(&src, path.join("config.toml"))
                    .with_context(|| format!("Copying {}", src.display()))?;
            }
        }
    }
    Ok(path)
}

pub fn import_default(name: Option<&str>) -> Result<(String, PathBuf)> {
    let paths = config::init()?;
    let name = match name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => default_profile_name()?,
    };
    config::ensure_profile_name(name.as_str())?;
    let source = default_codex_home()?;
    if !source.is_dir() {
        anyhow::bail!("Default Codex home does not exist: {}", source.display());
    }
    let target = paths.profiles.join(&name);
    if target.exists() {
        anyhow::bail!("Profile already exists: {name}");
    }

    fs::create_dir_all(&target).with_context(|| format!("Creating {}", target.display()))?;
    if let Err(err) = copy_codex_home(&source, &target) {
        fs::remove_dir_all(&target).ok();
        return Err(err);
    }
    fs::create_dir_all(target.join("sessions"))
        .with_context(|| format!("Creating {}", target.join("sessions").display()))?;
    Ok((name, target))
}

pub fn import_sub2_json(source: impl AsRef<Path>, name: Option<&str>) -> Result<(String, PathBuf)> {
    let source = source.as_ref();
    let data =
        fs::read_to_string(source).with_context(|| format!("Reading {}", source.display()))?;
    let export: Sub2Export = serde_json::from_str(&data)
        .with_context(|| format!("Parsing sub2 JSON {}", source.display()))?;
    let account = export
        .accounts
        .into_iter()
        .find(|account| account.platform.as_deref().unwrap_or("openai") == "openai")
        .context("sub2 JSON does not contain an OpenAI account")?;
    let name = match name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => account
            .credentials
            .email
            .clone()
            .or_else(|| account.extra.email.clone())
            .or_else(|| email_like(&account.name))
            .context("sub2 account does not contain an email address; pass a profile name")?,
    };
    config::ensure_profile_name(name.as_str())?;

    let paths = config::init()?;
    let target = paths.profiles.join(&name);
    if target.exists() {
        anyhow::bail!("Profile already exists: {name}");
    }
    fs::create_dir_all(target.join("sessions"))
        .with_context(|| format!("Creating {}", target.display()))?;
    if let Some(src) = config::default_codex_config() {
        if src.exists() {
            fs::copy(&src, target.join("config.toml"))
                .with_context(|| format!("Copying {}", src.display()))?;
        }
    }

    let auth = json!({
        "OPENAI_API_KEY": Value::Null,
        "tokens": {
            "id_token": account.credentials.id_token,
            "access_token": account.credentials.access_token,
            "refresh_token": account.credentials.refresh_token,
            "account_id": account.credentials.account_id,
        },
        "plan_expires_at": account.credentials.expires_at,
        "plan_type": account.credentials.plan_type,
        "last_refresh": account.extra.last_refresh.or(export.exported_at),
    });
    fs::write(target.join("auth.json"), serde_json::to_vec_pretty(&auth)?)
        .with_context(|| format!("Writing {}", target.join("auth.json").display()))?;
    secure_file(target.join("auth.json"))?;
    secure_file(target.join("config.toml")).ok();
    Ok((name, target))
}

pub fn delete(name: &str) -> Result<()> {
    let path = profile_path(name)?;
    if !path.exists() {
        anyhow::bail!("Profile does not exist: {name}");
    }
    fs::remove_dir_all(&path).with_context(|| format!("Deleting {}", path.display()))?;
    Ok(())
}

pub fn copy_session_to_profile(
    source_profile: &str,
    target_profile: &str,
    session_id: &str,
    source_session_path: &str,
) -> Result<PathBuf> {
    let source_root = ensure_exists(source_profile)?;
    let target_root = ensure_exists(target_profile)?;
    let source_session_path = PathBuf::from(source_session_path);
    if !source_session_path.is_file() {
        anyhow::bail!(
            "Session file does not exist: {}",
            source_session_path.display()
        );
    }
    let relative = source_session_path
        .strip_prefix(&source_root)
        .with_context(|| {
            format!(
                "Session file {} is not under source profile {}",
                source_session_path.display(),
                source_root.display()
            )
        })?;
    let target_session_path = target_root.join(relative);
    if let Some(parent) = target_session_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
    }
    fs::copy(&source_session_path, &target_session_path).with_context(|| {
        format!(
            "Copying {} to {}",
            source_session_path.display(),
            target_session_path.display()
        )
    })?;
    copy_session_index_line(&source_root, &target_root, session_id)?;
    Ok(target_session_path)
}

fn copy_codex_home(source: &Path, target: &Path) -> Result<()> {
    for entry in fs::read_dir(source).with_context(|| format!("Reading {}", source.display()))? {
        let entry = entry?;
        let name = entry.file_name();
        if name == "tmp" {
            continue;
        }
        copy_entry(&entry.path(), &target.join(name))?;
    }
    Ok(())
}

fn copy_entry(source: &Path, target: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(source).with_context(|| format!("Reading {}", source.display()))?;
    if metadata.file_type().is_symlink() {
        let link_target = fs::read_link(source)
            .with_context(|| format!("Reading symlink {}", source.display()))?;
        create_symlink(&link_target, target).with_context(|| {
            format!("Linking {} -> {}", target.display(), link_target.display())
        })?;
    } else if metadata.is_dir() {
        fs::create_dir_all(target).with_context(|| format!("Creating {}", target.display()))?;
        for entry in
            fs::read_dir(source).with_context(|| format!("Reading {}", source.display()))?
        {
            let entry = entry?;
            copy_entry(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
        }
        fs::copy(source, target)
            .with_context(|| format!("Copying {} to {}", source.display(), target.display()))?;
        fs::set_permissions(target, metadata.permissions())
            .with_context(|| format!("Setting permissions on {}", target.display()))?;
    }
    Ok(())
}

fn copy_session_index_line(source_root: &Path, target_root: &Path, session_id: &str) -> Result<()> {
    let source = source_root.join("session_index.jsonl");
    if !source.exists() {
        return Ok(());
    }
    let target = target_root.join("session_index.jsonl");
    let existing = fs::read_to_string(&target).unwrap_or_default();
    if existing.lines().any(|line| line.contains(session_id)) {
        return Ok(());
    }
    let source_data =
        fs::read_to_string(&source).with_context(|| format!("Reading {}", source.display()))?;
    let Some(line) = source_data.lines().find(|line| line.contains(session_id)) else {
        return Ok(());
    };
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target)
        .with_context(|| format!("Opening {}", target.display()))?;
    writeln!(file, "{line}").with_context(|| format!("Writing {}", target.display()))?;
    Ok(())
}

#[cfg(unix)]
fn secure_file(path: impl AsRef<Path>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let path = path.as_ref();
    if path.exists() {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Setting permissions on {}", path.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_file(_path: impl AsRef<Path>) -> Result<()> {
    Ok(())
}

#[derive(Debug, Deserialize)]
struct Sub2Export {
    exported_at: Option<String>,
    accounts: Vec<Sub2Account>,
}

#[derive(Debug, Deserialize)]
struct Sub2Account {
    name: String,
    platform: Option<String>,
    credentials: Sub2Credentials,
    #[serde(default)]
    extra: Sub2Extra,
}

#[derive(Debug, Deserialize)]
struct Sub2Credentials {
    access_token: String,
    refresh_token: String,
    id_token: String,
    account_id: String,
    email: Option<String>,
    expires_at: Option<i64>,
    plan_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Sub2Extra {
    email: Option<String>,
    last_refresh: Option<String>,
}

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        std::os::windows::fs::symlink_dir(source, target)
    } else {
        std::os::windows::fs::symlink_file(source, target)
    }
}

pub fn ensure_exists(name: &str) -> Result<PathBuf> {
    let path = profile_path(name)?;
    if !path.is_dir() {
        anyhow::bail!("Profile does not exist: {name}");
    }
    Ok(path)
}

pub fn list() -> Result<Vec<ProfileInfo>> {
    let paths = config::init()?;
    let mut out = Vec::new();
    for entry in fs::read_dir(&paths.profiles).context("Reading profiles directory")? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        out.push(metadata(&name)?);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    sort_by_plan_expiry(&mut out);
    Ok(out)
}

pub fn metadata(name: &str) -> Result<ProfileInfo> {
    let path = ensure_exists(name)?;
    let auth = path.join("auth.json");
    let logged_in = auth.exists();
    let auth_json = read_auth_json(&auth);
    let auth_mtime = fs::metadata(&auth)
        .and_then(|m| m.modified())
        .ok()
        .map(DateTime::<Local>::from);
    let plan_type = auth_json
        .as_ref()
        .and_then(|value| value.pointer("/plan_type"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let plan_expires_at = auth_json.as_ref().and_then(plan_expires_at);
    let sessions_size = size::path_size(&path.join("sessions"))?;
    let logs_size = logs_size(&path)?;
    let total_size = size::path_size(&path)?;
    let shared_cache = crate::shared::ALLOWED_SHARED.iter().any(|item| {
        fs::symlink_metadata(path.join(item))
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    });
    Ok(ProfileInfo {
        name: name.to_string(),
        path,
        logged_in,
        auth_mtime,
        plan_type,
        limit_5h_remaining: None,
        limit_7day_remaining: None,
        plan_expires_at,
        sessions_size,
        logs_size,
        total_size,
        shared_cache,
    })
}

fn sort_by_plan_expiry(profiles: &mut [ProfileInfo]) {
    profiles.sort_by(|a, b| {
        match (a.plan_expires_at, b.plan_expires_at) {
            (Some(a_time), Some(b_time)) => a_time.timestamp().cmp(&b_time.timestamp()),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| a.name.cmp(&b.name))
    });
}

fn read_auth_json(path: &Path) -> Option<Value> {
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn plan_expires_at(value: &Value) -> Option<DateTime<Local>> {
    value
        .pointer("/plan_expires_at")
        .and_then(Value::as_i64)
        .or_else(|| {
            value
                .pointer("/tokens/access_token")
                .and_then(Value::as_str)
                .and_then(jwt_exp)
        })
        .and_then(|ts| DateTime::<chrono::Utc>::from_timestamp(ts, 0))
        .map(DateTime::<Local>::from)
}

fn jwt_exp(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let decoded = decode_base64_url(payload)?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    value.pointer("/exp").and_then(Value::as_i64)
}

fn decode_base64_url(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            b'=' => break,
            _ => return None,
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn logs_size(path: &std::path::Path) -> Result<u64> {
    let mut total = size::path_size(&path.join("log"))? + size::path_size(&path.join("logs"))?;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("logs_") && name.ends_with(".sqlite") {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

fn default_codex_home() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("Cannot determine home directory")?
        .join(".codex"))
}

fn default_profile_name() -> Result<String> {
    let auth = default_codex_home()?.join("auth.json");
    let data = fs::read_to_string(&auth).with_context(|| {
        format!(
            "Cannot derive default profile name because {} is missing or unreadable",
            auth.display()
        )
    })?;
    let value: Value = serde_json::from_str(&data).with_context(|| {
        format!(
            "Cannot parse {} to derive email profile name",
            auth.display()
        )
    })?;
    find_email_by_key(&value)
        .or_else(|| find_email_value(&value))
        .context(
            "Cannot find an email address in ~/.codex/auth.json; pass a profile name explicitly",
        )
}

fn find_email_by_key(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key.to_ascii_lowercase().contains("email") {
                    if let Some(text) = value.as_str().and_then(email_like) {
                        return Some(text);
                    }
                }
                if let Some(found) = find_email_by_key(value) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().find_map(find_email_by_key),
        _ => None,
    }
}

fn find_email_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => email_like(text),
        Value::Array(items) => items.iter().find_map(find_email_value),
        Value::Object(map) => map.values().find_map(find_email_value),
        _ => None,
    }
}

fn email_like(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.len() <= 254
        && trimmed.contains('@')
        && trimmed
            .rsplit_once('@')
            .is_some_and(|(_, domain)| domain.contains('.'))
        && !trimmed.chars().any(char::is_whitespace)
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
    {
        Some(trimmed.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn imports_sub2_json_as_isolated_codex_profile() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-sub2-test-{stamp}"));
        let source = root.join("sub2.json");
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("CODEXHUB_HOME", &root);
        fs::write(
            &source,
            r#"{
              "exported_at": "2026-05-27T17:34:20+08:00",
              "proxies": [],
              "accounts": [
                {
                  "name": "Example",
                  "platform": "openai",
                  "type": "oauth",
                  "credentials": {
                    "access_token": "access-value",
                    "refresh_token": "refresh-value",
                    "id_token": "id-value",
                    "account_id": "account-value",
                    "email": "person@example.com",
                    "expires_at": 1780716616,
                    "plan_type": "plus"
                  },
                  "extra": {
                    "last_refresh": "2026-05-27T17:34:20+08:00"
                  }
                }
              ]
            }"#,
        )
        .unwrap();

        let (name, path) = import_sub2_json(&source, None).unwrap();

        assert_eq!(name, "person@example.com");
        assert_eq!(path, root.join("profiles").join("person@example.com"));
        assert!(path.join("sessions").is_dir());
        let auth: Value =
            serde_json::from_str(&fs::read_to_string(path.join("auth.json")).unwrap()).unwrap();
        assert!(auth["OPENAI_API_KEY"].is_null());
        assert_eq!(auth["tokens"]["access_token"], "access-value");
        assert_eq!(auth["tokens"]["refresh_token"], "refresh-value");
        assert_eq!(auth["tokens"]["id_token"], "id-value");
        assert_eq!(auth["tokens"]["account_id"], "account-value");
        assert_eq!(auth["plan_expires_at"], 1780716616);
        assert_eq!(auth["plan_type"], "plus");
        assert_eq!(auth["last_refresh"], "2026-05-27T17:34:20+08:00");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn imports_sub2_json_with_explicit_name_without_email() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-sub2-named-test-{stamp}"));
        let source = root.join("sub2.json");
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("CODEXHUB_HOME", &root);
        fs::write(
            &source,
            r#"{
              "accounts": [
                {
                  "name": "No Email",
                  "platform": "openai",
                  "credentials": {
                    "access_token": "access-value",
                    "refresh_token": "refresh-value",
                    "id_token": "id-value",
                    "account_id": "account-value"
                  }
                }
              ]
            }"#,
        )
        .unwrap();

        let (name, path) = import_sub2_json(&source, Some("named-profile")).unwrap();

        assert_eq!(name, "named-profile");
        assert!(path.join("auth.json").is_file());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn sorts_profiles_by_plan_expiry_with_unknown_last() {
        let mut profiles = vec![
            profile_for_sort("unknown", None),
            profile_for_sort("late", Some(200)),
            profile_for_sort("early", Some(100)),
        ];

        sort_by_plan_expiry(&mut profiles);

        let names: Vec<_> = profiles.into_iter().map(|p| p.name).collect();
        assert_eq!(names, vec!["early", "late", "unknown"]);
    }

    #[test]
    fn reads_plan_expiry_from_access_token_exp_fallback() {
        let value = json!({
            "tokens": {
                "access_token": "header.eyJleHAiOjE3ODA3MTY2MTZ9.signature"
            }
        });

        let expires = plan_expires_at(&value).unwrap();

        assert_eq!(expires.timestamp(), 1780716616);
    }

    #[test]
    fn copies_session_file_and_index_to_target_profile() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-copy-session-test-{stamp}"));
        std::env::set_var("CODEXHUB_HOME", &root);
        let source = create("source", false).unwrap();
        let target = create("target", false).unwrap();
        let session = source
            .join("sessions")
            .join("2026")
            .join("05")
            .join("28")
            .join("rollout-test-session.jsonl");
        fs::create_dir_all(session.parent().unwrap()).unwrap();
        fs::write(&session, "{}\n").unwrap();
        fs::write(
            source.join("session_index.jsonl"),
            r#"{"id":"test-session","thread_name":"Test","updated_at":"2026-05-28T00:00:00Z"}"#,
        )
        .unwrap();

        let copied = copy_session_to_profile(
            "source",
            "target",
            "test-session",
            session.to_str().unwrap(),
        )
        .unwrap();

        assert_eq!(copied, target.join(session.strip_prefix(&source).unwrap()));
        assert_eq!(fs::read_to_string(copied).unwrap(), "{}\n");
        assert!(fs::read_to_string(target.join("session_index.jsonl"))
            .unwrap()
            .contains("test-session"));

        fs::remove_dir_all(&root).ok();
    }

    fn profile_for_sort(name: &str, expiry: Option<i64>) -> ProfileInfo {
        ProfileInfo {
            name: name.to_string(),
            path: PathBuf::new(),
            logged_in: true,
            auth_mtime: None,
            plan_type: None,
            limit_5h_remaining: None,
            limit_7day_remaining: None,
            plan_expires_at: expiry
                .and_then(|ts| DateTime::<chrono::Utc>::from_timestamp(ts, 0))
                .map(DateTime::<Local>::from),
            sessions_size: 0,
            logs_size: 0,
            total_size: 0,
            shared_cache: false,
        }
    }
}
