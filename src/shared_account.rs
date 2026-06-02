use crate::{config, profile};
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SHARED_ACCOUNTS_ROOT: &str = "/var/lib/codexhub/shared-accounts";

#[derive(Debug, Clone)]
pub struct ShareAccountPlan {
    pub profile: String,
    pub target_user: String,
    pub source_profile: PathBuf,
    pub shared_profile: PathBuf,
    pub needs_sudo: bool,
}

#[derive(Debug, Clone)]
pub struct SharedAccountInfo {
    pub name: String,
    pub path: PathBuf,
    pub owner: Option<String>,
    pub allowed_users: Vec<String>,
    pub created_at: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SharedAccountManifest {
    name: String,
    owner: String,
    created_at: String,
    allowed_users: Vec<String>,
    shared_files: Vec<String>,
}

pub fn plan_share_account(profile_name: &str, target_user: &str) -> Result<ShareAccountPlan> {
    config::ensure_profile_name(profile_name)?;
    ensure_user_name(target_user)?;
    let source_profile = profile::ensure_exists(profile_name)?;
    ensure_shareable_file(&source_profile.join("auth.json"))?;
    Ok(ShareAccountPlan {
        profile: profile_name.to_string(),
        target_user: target_user.to_string(),
        source_profile,
        shared_profile: shared_profile_path(profile_name),
        needs_sudo: !shared_root_writable(),
    })
}

pub fn share_account(profile_name: &str, target_user: &str) -> Result<()> {
    let plan = plan_share_account(profile_name, target_user)?;
    if plan.needs_sudo {
        anyhow::bail!("{}", sudo_command(&plan));
    }
    write_shared_account(&plan)
}

pub fn setup_shared_account(
    profile_name: &str,
    target_user: &str,
    source_profile: Option<PathBuf>,
) -> Result<()> {
    let mut plan = match source_profile {
        Some(source_profile) => {
            config::ensure_profile_name(profile_name)?;
            ensure_user_name(target_user)?;
            ensure_shareable_file(&source_profile.join("auth.json"))?;
            ShareAccountPlan {
                profile: profile_name.to_string(),
                target_user: target_user.to_string(),
                source_profile,
                shared_profile: shared_profile_path(profile_name),
                needs_sudo: false,
            }
        }
        None => plan_share_account(profile_name, target_user)?,
    };
    plan.needs_sudo = false;
    write_shared_account(&plan)
}

pub fn list_shared_accounts() -> Result<Vec<SharedAccountInfo>> {
    let root = Path::new(SHARED_ACCOUNTS_ROOT);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(Vec::new());
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry
            .file_type()
            .map(|file_type| file_type.is_dir())
            .unwrap_or(false)
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry.path().join("auth.json").is_file() {
            continue;
        }
        out.push(shared_account_info(name, entry.path()));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn import_shared_account(name: &str) -> Result<PathBuf> {
    config::ensure_profile_name(name)?;
    let shared = shared_profile_path(name);
    ensure_shareable_file(&shared.join("auth.json"))?;
    let paths = config::init()?;
    let target = paths.profiles.join(name);
    if target.exists() {
        anyhow::bail!("Profile already exists: {name}");
    }
    fs::create_dir_all(&target).with_context(|| format!("Creating {}", target.display()))?;
    create_symlink(&shared.join("auth.json"), &target.join("auth.json"))
        .with_context(|| format!("Linking shared auth for {name}"))?;
    if shared.join("config.toml").is_file() {
        create_symlink(&shared.join("config.toml"), &target.join("config.toml"))
            .with_context(|| format!("Linking shared config for {name}"))?;
    }
    fs::create_dir_all(target.join("sessions"))
        .with_context(|| format!("Creating {}", target.join("sessions").display()))?;
    Ok(target)
}

pub fn sudo_command(plan: &ShareAccountPlan) -> String {
    format!(
        "sudo {} shared-account setup {} {} --source-profile {}",
        shell_escape(current_exe()),
        shell_escape(&plan.profile),
        shell_escape(&plan.target_user),
        shell_escape(plan.source_profile.display().to_string())
    )
}

pub fn tmux_helper_command(plan: &ShareAccountPlan) -> String {
    format!(
        "tmux new-session -A -s codexhub-share-{} {}",
        sanitize_tmux_name(&plan.profile),
        shell_escape(format!(
            "{}; echo; echo 'Shared account setup finished. Press Enter to close.'; read _",
            sudo_command(plan)
        ))
    )
}

fn write_shared_account(plan: &ShareAccountPlan) -> Result<()> {
    fs::create_dir_all(&plan.shared_profile)
        .with_context(|| format!("Creating {}", plan.shared_profile.display()))?;
    fs::copy(
        plan.source_profile.join("auth.json"),
        plan.shared_profile.join("auth.json"),
    )
    .with_context(|| format!("Copying auth.json for {}", plan.profile))?;
    if plan.source_profile.join("config.toml").is_file() {
        fs::copy(
            plan.source_profile.join("config.toml"),
            plan.shared_profile.join("config.toml"),
        )
        .with_context(|| format!("Copying config.toml for {}", plan.profile))?;
    }
    let manifest = SharedAccountManifest {
        name: plan.profile.clone(),
        owner: current_user(),
        created_at: Local::now().to_rfc3339(),
        allowed_users: vec![plan.target_user.clone()],
        shared_files: vec!["auth.json".into(), "config.toml".into()],
    };
    fs::write(
        plan.shared_profile.join("manifest.toml"),
        toml::to_string_pretty(&manifest)?,
    )
    .with_context(|| format!("Writing manifest for {}", plan.profile))?;
    set_acl(&plan.shared_profile, &plan.target_user)?;
    Ok(())
}

fn set_acl(path: &Path, target_user: &str) -> Result<()> {
    run_setfacl(Path::new("/var/lib/codexhub"), target_user, "rx")?;
    run_setfacl(Path::new(SHARED_ACCOUNTS_ROOT), target_user, "rx")?;
    run_setfacl(path, target_user, "rx")?;
    run_setfacl(&path.join("auth.json"), target_user, "rw")?;
    if path.join("config.toml").exists() {
        run_setfacl(&path.join("config.toml"), target_user, "r")?;
    }
    Ok(())
}

fn run_setfacl(path: &Path, target_user: &str, perms: &str) -> Result<()> {
    let status = Command::new("setfacl")
        .args(["-m", &format!("u:{target_user}:{perms}")])
        .arg(path)
        .status()
        .with_context(|| "Starting setfacl")?;
    if !status.success() {
        anyhow::bail!("setfacl failed with status {status}");
    }
    Ok(())
}

fn shared_account_info(name: String, path: PathBuf) -> SharedAccountInfo {
    let manifest = fs::read_to_string(path.join("manifest.toml"))
        .ok()
        .and_then(|data| toml::from_str::<SharedAccountManifest>(&data).ok());
    SharedAccountInfo {
        name,
        path,
        owner: manifest.as_ref().map(|manifest| manifest.owner.clone()),
        allowed_users: manifest
            .as_ref()
            .map(|manifest| manifest.allowed_users.clone())
            .unwrap_or_default(),
        created_at: manifest
            .as_ref()
            .and_then(|manifest| DateTime::parse_from_rfc3339(&manifest.created_at).ok())
            .map(DateTime::<Local>::from),
    }
}

fn shared_profile_path(profile_name: &str) -> PathBuf {
    Path::new(SHARED_ACCOUNTS_ROOT).join(profile_name)
}

fn shared_root_writable() -> bool {
    let root = Path::new(SHARED_ACCOUNTS_ROOT);
    if root.exists() {
        return root.is_dir()
            && fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join(".codexhub-write-test"))
                .map(|_| {
                    fs::remove_file(root.join(".codexhub-write-test")).ok();
                })
                .is_ok();
    }
    fs::create_dir_all(root).is_ok()
}

fn ensure_shareable_file(path: &Path) -> Result<()> {
    if !path.is_file() {
        anyhow::bail!("Required file does not exist: {}", path.display());
    }
    Ok(())
}

fn ensure_user_name(value: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        anyhow::bail!("Invalid Linux user name: {value}");
    }
    Ok(())
}

fn current_exe() -> String {
    std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "codexhub".into())
}

fn current_user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "unknown".into())
}

fn sanitize_tmux_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn shell_escape(value: impl AsRef<str>) -> String {
    let text = value.as_ref();
    format!("'{}'", text.replace('\'', r#"'\''"#))
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
