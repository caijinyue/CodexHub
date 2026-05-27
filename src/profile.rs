use crate::config;
use crate::size;
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProfileInfo {
    pub name: String,
    pub path: PathBuf,
    pub logged_in: bool,
    pub auth_mtime: Option<DateTime<Local>>,
    pub sessions_size: u64,
    pub logs_size: u64,
    pub total_size: u64,
    pub shared_cache: bool,
}

pub fn profile_path(name: &str) -> Result<PathBuf> {
    config::ensure_profile_name(&name)?;
    Ok(config::paths()?.profiles.join(name))
}

pub fn create(name: &str, copy_config: bool) -> Result<PathBuf> {
    let paths = config::init()?;
    config::ensure_profile_name(&name)?;
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

pub fn delete(name: &str) -> Result<()> {
    let path = profile_path(name)?;
    if !path.exists() {
        anyhow::bail!("Profile does not exist: {name}");
    }
    fs::remove_dir_all(&path).with_context(|| format!("Deleting {}", path.display()))?;
    Ok(())
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

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
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
    Ok(out)
}

pub fn metadata(name: &str) -> Result<ProfileInfo> {
    let path = ensure_exists(name)?;
    let auth = path.join("auth.json");
    let logged_in = auth.exists();
    let auth_mtime = fs::metadata(&auth)
        .and_then(|m| m.modified())
        .ok()
        .map(DateTime::<Local>::from);
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
        sessions_size,
        logs_size,
        total_size,
        shared_cache,
    })
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
