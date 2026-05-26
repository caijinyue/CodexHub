use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub profiles: PathBuf,
    pub shared: PathBuf,
    pub backups: PathBuf,
    pub logs: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub shared_cache: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            shared_cache: crate::shared::ALLOWED_SHARED
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

pub fn paths() -> Result<Paths> {
    let root = match env::var_os("CODEXHUB_HOME") {
        Some(path) => expand_tilde(PathBuf::from(path)),
        None => dirs::home_dir()
            .context("Cannot determine home directory")?
            .join(".codexhub"),
    };
    Ok(Paths {
        config: root.join("config.toml"),
        profiles: root.join("profiles"),
        shared: root.join("shared"),
        backups: root.join("backups"),
        logs: root.join("logs"),
        root,
    })
}

pub fn init() -> Result<Paths> {
    let paths = paths()?;
    fs::create_dir_all(&paths.profiles).context("Creating profiles directory")?;
    fs::create_dir_all(&paths.shared).context("Creating shared directory")?;
    fs::create_dir_all(&paths.backups).context("Creating backups directory")?;
    fs::create_dir_all(&paths.logs).context("Creating logs directory")?;
    for item in crate::shared::ALLOWED_SHARED {
        let target = paths.shared.join(item);
        if item.ends_with(".json") {
            if !target.exists() {
                fs::write(&target, b"{}\n")
                    .with_context(|| format!("Creating {}", target.display()))?;
            }
        } else {
            fs::create_dir_all(&target)
                .with_context(|| format!("Creating {}", target.display()))?;
        }
    }
    if !paths.config.exists() {
        let data = toml::to_string_pretty(&Config::default())?;
        fs::write(&paths.config, data).context("Writing config.toml")?;
    }
    Ok(paths)
}

pub fn expand_tilde(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if text == "~" {
        return dirs::home_dir().unwrap_or(path);
    }
    if let Some(rest) = text.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path
}

pub fn default_codex_config() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".codex").join("config.toml"))
}

pub fn ensure_profile_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(|c| c.is_control())
    {
        anyhow::bail!("Invalid profile name: {name}");
    }
    Ok(())
}

pub fn is_broken_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink() && fs::metadata(path).is_err())
        .unwrap_or(false)
}
