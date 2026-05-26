use crate::config;
use crate::size;
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use std::fs;
use std::path::PathBuf;

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

pub fn delete(name: &str) -> Result<()> {
    let path = profile_path(name)?;
    if !path.exists() {
        anyhow::bail!("Profile does not exist: {name}");
    }
    fs::remove_dir_all(&path).with_context(|| format!("Deleting {}", path.display()))?;
    Ok(())
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
