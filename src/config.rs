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
    #[serde(default = "default_quota_refresh_secs")]
    pub quota_refresh_secs: u64,
    #[serde(default)]
    pub proxy: ProxyConfig,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    Inherit,
    Off,
    Custom,
}

impl std::fmt::Display for ProxyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Inherit => "inherit",
            Self::Off => "off",
            Self::Custom => "custom",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyConfig {
    #[serde(default = "default_proxy_mode")]
    pub mode: ProxyMode,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub http: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub https: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub all: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub no_proxy: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            mode: default_proxy_mode(),
            http: String::new(),
            https: String::new(),
            all: String::new(),
            no_proxy: "localhost,127.0.0.1,::1".into(),
        }
    }
}

fn default_proxy_mode() -> ProxyMode {
    ProxyMode::Inherit
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            shared_cache: crate::shared::ALLOWED_SHARED
                .iter()
                .map(|s| s.to_string())
                .collect(),
            quota_refresh_secs: default_quota_refresh_secs(),
            proxy: ProxyConfig::default(),
        }
    }
}

pub fn default_quota_refresh_secs() -> u64 {
    60
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
    secure_config_file(&paths.config)?;
    Ok(paths)
}

pub fn load() -> Result<Config> {
    let paths = init()?;
    let data = fs::read_to_string(&paths.config).context("Reading config.toml")?;
    let mut config: Config = toml::from_str(&data).context("Parsing config.toml")?;
    if config.quota_refresh_secs == 0 {
        config.quota_refresh_secs = default_quota_refresh_secs();
    }
    Ok(config)
}

pub fn save(config: &Config) -> Result<()> {
    let paths = init()?;
    let data = toml::to_string_pretty(config)?;
    fs::write(&paths.config, data).context("Writing config.toml")?;
    secure_config_file(&paths.config)
}

#[cfg(unix)]
fn secure_config_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .context("Securing config.toml permissions")
}

#[cfg(not(unix))]
fn secure_config_file(_path: &Path) -> Result<()> {
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_without_proxy_uses_inherit_mode() {
        let config: Config = toml::from_str(
            r#"
version = 1
shared_cache = []
quota_refresh_secs = 60
"#,
        )
        .unwrap();
        assert_eq!(config.proxy.mode, ProxyMode::Inherit);
        assert_eq!(config.proxy.no_proxy, "localhost,127.0.0.1,::1");
    }

    #[test]
    fn proxy_config_round_trips() {
        let mut config = Config::default();
        config.proxy.mode = ProxyMode::Custom;
        config.proxy.http = "http://127.0.0.1:27183".into();
        config.proxy.all = "socks5://127.0.0.1:27183".into();
        let encoded = toml::to_string(&config).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded.proxy, config.proxy);
    }
}
