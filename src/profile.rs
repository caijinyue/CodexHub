use crate::config;
use crate::size;
use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProfileInfo {
    pub name: String,
    pub path: PathBuf,
    pub logged_in: bool,
    pub auth_mtime: Option<DateTime<Local>>,
    pub plan_type: Option<String>,
    pub limit_5h_label: String,
    pub limit_5h_remaining: Option<u8>,
    pub limit_7day_label: String,
    pub limit_7day_remaining: Option<u8>,
    pub limit_5h_resets_at: Option<DateTime<Local>>,
    pub limit_7day_resets_at: Option<DateTime<Local>>,
    pub plan_expires_at: Option<DateTime<Local>>,
    pub used_since: Option<DateTime<Local>>,
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
        self.limit_5h_label = status.primary_label;
        self.limit_5h_remaining = status.primary_remaining_percent;
        self.limit_7day_label = status.secondary_label;
        self.limit_7day_remaining = status.secondary_remaining_percent;
        self.limit_5h_resets_at = status.primary_resets_at;
        self.limit_7day_resets_at = status.secondary_resets_at;
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
        copy_default_codex_config(&path)?;
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
    let account_id = account
        .credentials
        .account_id
        .clone()
        .or_else(|| account.credentials.chatgpt_account_id.clone())
        .or_else(|| account_id_from_id_token(&account.credentials.access_token))
        .or_else(|| {
            account
                .credentials
                .id_token
                .as_deref()
                .and_then(account_id_from_id_token)
        });

    let paths = config::init()?;
    let target = paths.profiles.join(&name);
    if target.exists() {
        if target.join("auth.json").exists() {
            anyhow::bail!("Profile already exists: {name}");
        }
        if !target.is_dir() {
            anyhow::bail!(
                "Profile path exists but is not a directory: {}",
                target.display()
            );
        }
    } else {
        fs::create_dir_all(&target).with_context(|| format!("Creating {}", target.display()))?;
    }
    fs::create_dir_all(target.join("sessions"))
        .with_context(|| format!("Creating {}", target.display()))?;
    if !target.join("config.toml").is_file() {
        copy_default_codex_config(&target)?;
    }

    let auth = json!({
        "OPENAI_API_KEY": Value::Null,
        "tokens": {
            "id_token": account.credentials.id_token.unwrap_or_default(),
            "access_token": account.credentials.access_token,
            "refresh_token": account.credentials.refresh_token.unwrap_or_default(),
            "account_id": account_id.unwrap_or_default(),
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
    for file in ["auth.json", ".credentials.json", "installation_id"] {
        let target = path.join(file);
        if target.exists() {
            fs::remove_file(&target).with_context(|| format!("Deleting {}", target.display()))?;
        }
    }
    Ok(())
}

pub fn copy_session_to_profile(
    source_profile: &str,
    target_profile: &str,
    session_id: &str,
    source_session_path: &str,
) -> Result<PathBuf> {
    let source_root = ensure_exists(source_profile)?;
    copy_session_root_to_profile(
        &source_root,
        target_profile,
        session_id,
        source_session_path,
    )
}

pub fn copy_session_root_to_profile(
    source_root: &Path,
    target_profile: &str,
    session_id: &str,
    source_session_path: &str,
) -> Result<PathBuf> {
    let target_root = ensure_exists(target_profile)?;
    let source_session_path = PathBuf::from(source_session_path);
    if !source_session_path.is_file() {
        anyhow::bail!(
            "Session file does not exist: {}",
            source_session_path.display()
        );
    }
    let lineage = session_rollout_lineage(source_root, session_id, &source_session_path)?;
    let target_session_path = target_root.join(
        source_session_path
            .strip_prefix(source_root)
            .with_context(|| {
                format!(
                    "Session file {} is not under source home {}",
                    source_session_path.display(),
                    source_root.display()
                )
            })?,
    );

    // Copy ancestors first so a resumable fork is never published without the
    // source rollouts referenced by its paginated history_base chain.
    for (lineage_session_id, lineage_path) in lineage.iter().rev() {
        let relative = lineage_path.strip_prefix(source_root).with_context(|| {
            format!(
                "Session file {} is not under source home {}",
                lineage_path.display(),
                source_root.display()
            )
        })?;
        let target_path = target_root.join(relative);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
        }
        crate::text_encoding::copy_as_utf8(lineage_path, &target_path)?;
        copy_session_index_line(source_root, &target_root, lineage_session_id)?;
    }
    Ok(target_session_path)
}

fn session_rollout_lineage(
    source_root: &Path,
    session_id: &str,
    source_session_path: &Path,
) -> Result<Vec<(String, PathBuf)>> {
    let mut lineage = Vec::new();
    let mut seen = HashSet::new();
    let mut current_id = session_id.to_string();
    let mut current_path = source_session_path.to_path_buf();

    loop {
        if !seen.insert(current_id.clone()) {
            anyhow::bail!(
                "Invalid paginated history lineage for {session_id}: cycle at {current_id}"
            );
        }
        let history_base = session_history_base_thread_id(&current_path)?;
        lineage.push((current_id, current_path));
        let Some(parent_id) = history_base else {
            break;
        };
        let parent_path = find_session_rollout(source_root, &parent_id).with_context(|| {
            format!(
                "Invalid paginated history lineage for {session_id}: missing source rollout {parent_id}"
            )
        })?;
        current_id = parent_id;
        current_path = parent_path;
    }

    Ok(lineage)
}

fn session_history_base_thread_id(path: &Path) -> Result<Option<String>> {
    let data = crate::text_encoding::read_to_string(path)
        .with_context(|| format!("Reading session metadata from {}", path.display()))?;
    let Some(line) = data.lines().next() else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(line)
        .with_context(|| format!("Parsing session metadata from {}", path.display()))?;
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return Ok(None);
    }
    Ok(value
        .pointer("/payload/history_base/thread_id")
        .and_then(Value::as_str)
        .map(str::to_string))
}

fn find_session_rollout(source_root: &Path, session_id: &str) -> Option<PathBuf> {
    ["sessions", "archived_sessions"]
        .into_iter()
        .flat_map(|directory| {
            walkdir::WalkDir::new(source_root.join(directory))
                .into_iter()
                .filter_map(Result::ok)
        })
        .find_map(|entry| {
            let path = entry.path();
            if !path.is_file()
                || !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(session_id))
            {
                return None;
            }
            let data = crate::text_encoding::read_to_string(path).ok()?;
            let value: Value = serde_json::from_str(data.lines().next()?).ok()?;
            let metadata_id = value
                .pointer("/payload/session_id")
                .or_else(|| value.pointer("/payload/id"))
                .and_then(Value::as_str)?;
            (metadata_id == session_id).then(|| path.to_path_buf())
        })
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

fn copy_default_codex_config(target: &Path) -> Result<()> {
    let Some(source) = config::default_codex_config().filter(|path| path.is_file()) else {
        return Ok(());
    };
    copy_codex_config(&source, target)
}

fn copy_codex_config(source: &Path, target: &Path) -> Result<()> {
    fs::copy(source, target.join("config.toml"))
        .with_context(|| format!("Copying {}", source.display()))?;

    let data =
        fs::read_to_string(source).with_context(|| format!("Reading {}", source.display()))?;
    let config: toml::Value =
        toml::from_str(&data).with_context(|| format!("Parsing {}", source.display()))?;
    let Some(relative) = config
        .get("model_catalog_json")
        .and_then(toml::Value::as_str)
    else {
        return Ok(());
    };
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Ok(());
    }
    let Some(source_dir) = source.parent() else {
        return Ok(());
    };
    let source_catalog = source_dir.join(relative);
    if !source_catalog.is_file() {
        return Ok(());
    }
    let target_catalog = target.join(relative);
    if let Some(parent) = target_catalog.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
    }
    fs::copy(&source_catalog, &target_catalog).with_context(|| {
        format!(
            "Copying {} to {}",
            source_catalog.display(),
            target_catalog.display()
        )
    })?;
    Ok(())
}

fn copy_session_index_line(source_root: &Path, target_root: &Path, session_id: &str) -> Result<()> {
    let source = source_root.join("session_index.jsonl");
    if !source.exists() {
        return Ok(());
    }
    let target = target_root.join("session_index.jsonl");
    let existing = crate::text_encoding::read_to_string(&target).unwrap_or_default();
    if existing.lines().any(|line| line.contains(session_id)) {
        return Ok(());
    }
    let source_data = crate::text_encoding::read_to_string(&source)?;
    let Some(line) = source_data.lines().find(|line| line.contains(session_id)) else {
        return Ok(());
    };
    let line = crate::text_encoding::repair_mojibake(line);
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
    refresh_token: Option<String>,
    id_token: Option<String>,
    account_id: Option<String>,
    chatgpt_account_id: Option<String>,
    email: Option<String>,
    expires_at: Option<Sub2Expiry>,
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum Sub2Expiry {
    Unix(i64),
    Rfc3339(String),
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
    let used_since = fs::metadata(&auth)
        .and_then(|m| m.created().or_else(|_| m.modified()))
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
        limit_5h_label: "primary".into(),
        limit_5h_remaining: None,
        limit_7day_label: "secondary".into(),
        limit_7day_remaining: None,
        limit_5h_resets_at: None,
        limit_7day_resets_at: None,
        plan_expires_at,
        used_since,
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
    [
        "/plan_expires_at",
        "/expires_at",
        "/account/plan_expires_at",
        "/account/expires_at",
        "/tokens/plan_expires_at",
    ]
    .into_iter()
    .filter_map(|path| value.pointer(path))
    .find_map(datetime_from_value)
}

fn datetime_from_value(value: &Value) -> Option<DateTime<Local>> {
    if let Some(timestamp) = value.as_i64() {
        return DateTime::<chrono::Utc>::from_timestamp(timestamp, 0).map(DateTime::<Local>::from);
    }
    let text = value.as_str()?;
    DateTime::parse_from_rfc3339(text)
        .map(DateTime::<Local>::from)
        .ok()
}

fn logs_size(path: &std::path::Path) -> Result<u64> {
    let mut total = size::path_size(&path.join("log"))? + size::path_size(&path.join("logs"))?;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("logs_") && name.ends_with(".sqlite") {
                match entry.metadata() {
                    Ok(metadata) => total += metadata.len(),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(err) => return Err(err.into()),
                }
            }
        }
    }
    Ok(total)
}

pub fn default_codex_home() -> Result<PathBuf> {
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

fn account_id_from_id_token(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    find_account_id_value(&value)
}

fn find_account_id_value(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => map.iter().find_map(|(key, value)| {
            if key.eq_ignore_ascii_case("account_id")
                || key.eq_ignore_ascii_case("accountId")
                || key.eq_ignore_ascii_case("chatgpt_account_id")
            {
                value.as_str().map(str::to_string)
            } else {
                find_account_id_value(value)
            }
        }),
        Value::Array(items) => items.iter().find_map(find_account_id_value),
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
    fn copies_relative_model_catalog_with_codex_config() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-config-copy-test-{stamp}"));
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(
            source.join("config.toml"),
            "model_catalog_json = \"catalogs/models.json\"\n",
        )
        .unwrap();
        fs::create_dir_all(source.join("catalogs")).unwrap();
        fs::write(source.join("catalogs/models.json"), "{}\n").unwrap();

        copy_codex_config(&source.join("config.toml"), &target).unwrap();

        assert!(target.join("config.toml").is_file());
        assert_eq!(
            fs::read_to_string(target.join("catalogs/models.json")).unwrap(),
            "{}\n"
        );
        fs::remove_dir_all(root).ok();
    }

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
    fn imports_sub2_json_with_rfc3339_expiry() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-sub2-rfc3339-test-{stamp}"));
        let source = root.join("sub2.json");
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("CODEXHUB_HOME", &root);
        fs::write(
            &source,
            r#"{
              "accounts": [
                {
                  "name": "person@example.com",
                  "platform": "openai",
                  "credentials": {
                    "access_token": "access-value",
                    "email": "person@example.com",
                    "chatgpt_account_id": "account-value",
                    "expires_at": "2026-07-16T09:32:30.000Z"
                  }
                }
              ]
            }"#,
        )
        .unwrap();

        let (_name, path) = import_sub2_json(&source, None).unwrap();
        let auth: Value =
            serde_json::from_str(&fs::read_to_string(path.join("auth.json")).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["refresh_token"], "");
        assert_eq!(auth["tokens"]["id_token"], "");
        assert_eq!(auth["tokens"]["account_id"], "account-value");
        assert_eq!(auth["plan_expires_at"], "2026-07-16T09:32:30.000Z");
        assert_eq!(
            plan_expires_at(&auth).map(|expiry| expiry.timestamp()),
            Some(1_784_194_350)
        );

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
    fn imports_sub2_json_repairs_existing_profile_without_auth() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-sub2-repair-test-{stamp}"));
        let source = root.join("sub2.json");
        let profile = root.join("profiles").join("person@example.com");
        fs::create_dir_all(profile.join("sessions")).unwrap();
        fs::write(profile.join("config.toml"), "approval_policy = \"never\"\n").unwrap();
        std::env::set_var("CODEXHUB_HOME", &root);
        fs::write(
            &source,
            r#"{
              "accounts": [
                {
                  "name": "person@example.com",
                  "platform": "openai",
                  "credentials": {
                    "access_token": "access-value",
                    "email": "person@example.com"
                  }
                }
              ]
            }"#,
        )
        .unwrap();

        let (_name, path) = import_sub2_json(&source, None).unwrap();

        let auth: Value =
            serde_json::from_str(&fs::read_to_string(path.join("auth.json")).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["access_token"], "access-value");
        assert_eq!(
            fs::read_to_string(path.join("config.toml")).unwrap(),
            "approval_policy = \"never\"\n"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn imports_sub2_json_without_account_id() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-sub2-no-account-id-test-{stamp}"));
        let source = root.join("sub2.json");
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("CODEXHUB_HOME", &root);
        fs::write(
            &source,
            r#"{
              "accounts": [
                {
                  "name": "person@example.com",
                  "platform": "openai",
                  "credentials": {
                    "access_token": "access-value",
                    "refresh_token": "refresh-value",
                    "id_token": "id-value",
                    "email": "person@example.com"
                  }
                }
              ]
            }"#,
        )
        .unwrap();

        let (_name, path) = import_sub2_json(&source, None).unwrap();
        let auth: Value =
            serde_json::from_str(&fs::read_to_string(path.join("auth.json")).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["account_id"], "");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn imports_sub2_json_chatgpt_account_id_as_account_id() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("codexhub-sub2-chatgpt-account-id-test-{stamp}"));
        let source = root.join("sub2.json");
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("CODEXHUB_HOME", &root);
        fs::write(
            &source,
            r#"{
              "accounts": [
                {
                  "name": "person@example.com",
                  "platform": "openai",
                  "credentials": {
                    "access_token": "access-value",
                    "refresh_token": "refresh-value",
                    "id_token": "id-value",
                    "chatgpt_account_id": "chatgpt-account-value",
                    "email": "person@example.com"
                  }
                }
              ]
            }"#,
        )
        .unwrap();

        let (_name, path) = import_sub2_json(&source, None).unwrap();
        let auth: Value =
            serde_json::from_str(&fs::read_to_string(path.join("auth.json")).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["account_id"], "chatgpt-account-value");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn imports_sub2_json_account_id_from_id_token() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("codexhub-sub2-id-token-account-test-{stamp}"));
        let source = root.join("sub2.json");
        let id_token = test_id_token(json!({
            "nested": {
                "account_id": "account-from-token"
            }
        }));
        fs::create_dir_all(&root).unwrap();
        std::env::set_var("CODEXHUB_HOME", &root);
        fs::write(
            &source,
            format!(
                r#"{{
                  "accounts": [
                    {{
                      "name": "person@example.com",
                      "platform": "openai",
                      "credentials": {{
                        "access_token": "access-value",
                        "refresh_token": "refresh-value",
                        "id_token": "{id_token}",
                        "email": "person@example.com"
                      }}
                    }}
                  ]
                }}"#
            ),
        )
        .unwrap();

        let (_name, path) = import_sub2_json(&source, None).unwrap();
        let auth: Value =
            serde_json::from_str(&fs::read_to_string(path.join("auth.json")).unwrap()).unwrap();
        assert_eq!(auth["tokens"]["account_id"], "account-from-token");

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
    fn ignores_access_token_exp_for_plan_expiry() {
        let value = json!({
            "tokens": {
                "access_token": "header.eyJleHAiOjE3ODA3MTY2MTZ9.signature"
            }
        });

        assert!(plan_expires_at(&value).is_none());
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

    #[test]
    fn copies_complete_paginated_session_lineage_to_target_profile() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-copy-lineage-test-{stamp}"));
        std::env::set_var("CODEXHUB_HOME", &root);
        let source = create("source", false).unwrap();
        let target = create("target", false).unwrap();
        let grandparent = source.join("sessions/2026/08/17/rollout-grandparent-session.jsonl");
        let parent = source.join("sessions/2026/08/18/rollout-parent-session.jsonl");
        let child = source.join("sessions/2026/08/19/rollout-child-session.jsonl");
        for path in [&grandparent, &parent, &child] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        fs::write(
            &grandparent,
            paginated_session_meta("grandparent-session", None),
        )
        .unwrap();
        fs::write(
            &parent,
            paginated_session_meta("parent-session", Some("grandparent-session")),
        )
        .unwrap();
        fs::write(
            &child,
            paginated_session_meta("child-session", Some("parent-session")),
        )
        .unwrap();
        fs::write(
            source.join("session_index.jsonl"),
            concat!(
                "{\"id\":\"grandparent-session\",\"thread_name\":\"Grandparent\"}\n",
                "{\"id\":\"parent-session\",\"thread_name\":\"Parent\"}\n",
                "{\"id\":\"child-session\",\"thread_name\":\"Child\"}\n"
            ),
        )
        .unwrap();

        let copied =
            copy_session_to_profile("source", "target", "child-session", child.to_str().unwrap())
                .unwrap();

        assert_eq!(copied, target.join(child.strip_prefix(&source).unwrap()));
        for source_path in [&grandparent, &parent, &child] {
            let target_path = target.join(source_path.strip_prefix(&source).unwrap());
            assert_eq!(
                fs::read_to_string(target_path).unwrap(),
                fs::read_to_string(source_path).unwrap()
            );
        }
        let target_index = fs::read_to_string(target.join("session_index.jsonl")).unwrap();
        for session_id in ["grandparent-session", "parent-session", "child-session"] {
            assert!(target_index.contains(session_id));
        }

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rejects_missing_paginated_parent_before_copying_child() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-missing-lineage-test-{stamp}"));
        std::env::set_var("CODEXHUB_HOME", &root);
        let source = create("source", false).unwrap();
        let target = create("target", false).unwrap();
        let child = source.join("sessions/2026/08/19/rollout-child-session.jsonl");
        fs::create_dir_all(child.parent().unwrap()).unwrap();
        fs::write(
            &child,
            paginated_session_meta("child-session", Some("missing-parent-session")),
        )
        .unwrap();

        let error =
            copy_session_to_profile("source", "target", "child-session", child.to_str().unwrap())
                .unwrap_err();

        assert!(error.to_string().contains("missing-parent-session"));
        assert!(!target.join(child.strip_prefix(&source).unwrap()).exists());

        fs::remove_dir_all(&root).ok();
    }

    fn paginated_session_meta(session_id: &str, history_base: Option<&str>) -> String {
        let history_base = history_base.map(|thread_id| {
            json!({
                "thread_id": thread_id,
                "end_ordinal_exclusive": 42,
                "end_byte_offset": 1024
            })
        });
        format!(
            "{}\n",
            json!({
                "timestamp": "2026-08-19T06:05:20.559Z",
                "ordinal": 42,
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "session_id": session_id,
                    "history_mode": "paginated",
                    "history_base": history_base
                }
            })
        )
    }

    #[test]
    fn copies_gbk_session_as_utf8() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-copy-gbk-session-test-{stamp}"));
        std::env::set_var("CODEXHUB_HOME", &root);
        let source = create("source", false).unwrap();
        let target = create("target", false).unwrap();
        let session = source
            .join("sessions")
            .join("2026")
            .join("06")
            .join("08")
            .join("rollout-test-session.jsonl");
        fs::create_dir_all(session.parent().unwrap()).unwrap();
        let rollout =
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"中文 prompt"}}"#;
        let (rollout_bytes, _, _) = encoding_rs::GBK.encode(rollout);
        fs::write(&session, rollout_bytes.as_ref()).unwrap();
        let index = r#"{"id":"test-session","thread_name":"ä¸­æ–‡ title","updated_at":"2026-06-08T00:00:00Z"}"#;
        fs::write(source.join("session_index.jsonl"), index).unwrap();

        let copied = copy_session_to_profile(
            "source",
            "target",
            "test-session",
            session.to_str().unwrap(),
        )
        .unwrap();

        assert_eq!(fs::read_to_string(copied).unwrap(), rollout);
        assert!(fs::read_to_string(target.join("session_index.jsonl"))
            .unwrap()
            .contains("中文 title"));

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn delete_removes_account_login_but_preserves_history() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-delete-account-test-{stamp}"));
        std::env::set_var("CODEXHUB_HOME", &root);
        let path = create("work", false).unwrap();
        fs::write(path.join("auth.json"), "{}\n").unwrap();
        fs::write(path.join(".credentials.json"), "{}\n").unwrap();
        fs::write(path.join("installation_id"), "install\n").unwrap();
        fs::write(path.join("history.jsonl"), "{}\n").unwrap();
        fs::write(path.join("session_index.jsonl"), "{}\n").unwrap();
        let session = path.join("sessions").join("rollout-test.jsonl");
        fs::write(&session, "{}\n").unwrap();

        delete("work").unwrap();

        assert!(path.is_dir());
        assert!(!path.join("auth.json").exists());
        assert!(!path.join(".credentials.json").exists());
        assert!(!path.join("installation_id").exists());
        assert!(path.join("history.jsonl").is_file());
        assert!(path.join("session_index.jsonl").is_file());
        assert!(session.is_file());

        fs::remove_dir_all(&root).ok();
    }

    fn profile_for_sort(name: &str, expiry: Option<i64>) -> ProfileInfo {
        ProfileInfo {
            name: name.to_string(),
            path: PathBuf::new(),
            logged_in: true,
            auth_mtime: None,
            plan_type: None,
            limit_5h_label: "primary".into(),
            limit_5h_remaining: None,
            limit_7day_label: "secondary".into(),
            limit_7day_remaining: None,
            limit_5h_resets_at: None,
            limit_7day_resets_at: None,
            plan_expires_at: expiry
                .and_then(|ts| DateTime::<chrono::Utc>::from_timestamp(ts, 0))
                .map(DateTime::<Local>::from),
            used_since: None,
            sessions_size: 0,
            logs_size: 0,
            total_size: 0,
            shared_cache: false,
        }
    }

    fn test_id_token(payload: Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        format!("{header}.{payload}.signature")
    }
}
