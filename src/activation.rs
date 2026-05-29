use crate::{config, profile};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationResult {
    pub profile_name: String,
    pub profile_path: PathBuf,
    pub current_profile_file: PathBuf,
    pub current_link: PathBuf,
    pub env_file: PathBuf,
    pub shell_file: PathBuf,
    pub environment_d_file: Option<PathBuf>,
}

pub fn activate_profile(name: &str) -> Result<ActivationResult> {
    activate_profile_impl(name, true)
}

pub fn active_profile_name() -> Result<Option<String>> {
    let file = config::paths()?.root.join("current_profile");
    let Ok(name) = fs::read_to_string(file) else {
        return Ok(None);
    };
    let name = name.trim();
    Ok((!name.is_empty()).then(|| name.to_string()))
}

#[cfg(test)]
fn activate_profile_for_test(name: &str) -> Result<ActivationResult> {
    activate_profile_impl(name, false)
}

fn activate_profile_impl(name: &str, publish: bool) -> Result<ActivationResult> {
    let paths = config::init()?;
    let profile_path = profile::ensure_exists(name)?;
    let current_profile_file = paths.root.join("current_profile");
    let current_link = paths.root.join("current");
    let env_file = paths.root.join("current.env");
    let shell_file = paths.root.join("activate.sh");
    let environment_d_file = environment_d_file();

    fs::write(&current_profile_file, format!("{name}\n"))
        .with_context(|| format!("Writing {}", current_profile_file.display()))?;
    replace_current_link(&profile_path, &current_link)?;
    write_env_file(&env_file, name, &profile_path)?;
    write_shell_file(&shell_file, name, &profile_path)?;
    if let Some(file) = &environment_d_file {
        write_env_file(file, name, &profile_path)?;
    }

    std::env::set_var("CODEX_HOME", &profile_path);
    std::env::set_var("CODEXHUB_PROFILE", name);
    if publish {
        publish_user_environment(name, &profile_path);
    }

    Ok(ActivationResult {
        profile_name: name.to_string(),
        profile_path,
        current_profile_file,
        current_link,
        env_file,
        shell_file,
        environment_d_file,
    })
}

fn write_env_file(path: &Path, name: &str, profile_path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
    }
    fs::write(
        path,
        format!(
            "CODEX_HOME={}\nCODEXHUB_PROFILE={name}\n",
            profile_path.display()
        ),
    )
    .with_context(|| format!("Writing {}", path.display()))
}

fn write_shell_file(path: &Path, name: &str, profile_path: &Path) -> Result<()> {
    fs::write(
        path,
        format!(
            "export CODEX_HOME={}\nexport CODEXHUB_PROFILE={}\n",
            shell_quote(&profile_path.to_string_lossy()),
            shell_quote(name)
        ),
    )
    .with_context(|| format!("Writing {}", path.display()))?;
    secure_executable(path)?;
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn environment_d_file() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("environment.d").join("10-codexhub.conf"))
}

#[cfg(unix)]
fn replace_current_link(profile_path: &Path, current_link: &Path) -> Result<()> {
    if fs::symlink_metadata(current_link).is_ok() {
        if current_link.is_dir() && !fs::symlink_metadata(current_link)?.file_type().is_symlink() {
            anyhow::bail!(
                "Cannot replace directory with current profile link: {}",
                current_link.display()
            );
        }
        fs::remove_file(current_link)
            .with_context(|| format!("Removing {}", current_link.display()))?;
    }
    std::os::unix::fs::symlink(profile_path, current_link).with_context(|| {
        format!(
            "Linking {} -> {}",
            current_link.display(),
            profile_path.display()
        )
    })
}

#[cfg(not(unix))]
fn replace_current_link(profile_path: &Path, current_link: &Path) -> Result<()> {
    fs::write(current_link, profile_path.to_string_lossy().as_bytes())
        .with_context(|| format!("Writing {}", current_link.display()))
}

#[cfg(unix)]
fn secure_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("Setting permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn secure_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn publish_user_environment(name: &str, profile_path: &Path) {
    #[cfg(target_os = "linux")]
    {
        let codex_home = format!("CODEX_HOME={}", profile_path.display());
        let profile = format!("CODEXHUB_PROFILE={name}");
        let _ = Command::new("systemctl")
            .args(["--user", "set-environment", &codex_home, &profile])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = Command::new("dbus-update-activation-environment")
            .args(["--systemd", &codex_home, &profile])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("launchctl")
            .args(["setenv", "CODEX_HOME", &profile_path.to_string_lossy()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = Command::new("launchctl")
            .args(["setenv", "CODEXHUB_PROFILE", name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (name, profile_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn writes_activation_files_for_profile() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-activate-test-{stamp}"));
        let config = root.join("xdg");
        std::env::set_var("CODEXHUB_HOME", &root);
        std::env::set_var("XDG_CONFIG_HOME", &config);

        let profile_path = crate::profile::create("work", false).unwrap();
        let result = activate_profile_for_test("work").unwrap();

        assert_eq!(result.profile_name, "work");
        assert_eq!(result.profile_path, profile_path);
        assert_eq!(
            fs::read_to_string(root.join("current_profile")).unwrap(),
            "work\n"
        );
        assert!(fs::read_to_string(root.join("current.env"))
            .unwrap()
            .contains(&format!("CODEX_HOME={}", profile_path.display())));
        assert!(fs::read_to_string(root.join("activate.sh"))
            .unwrap()
            .contains(&format!("export CODEX_HOME='{}'", profile_path.display())));
        assert!(
            fs::read_to_string(config.join("environment.d").join("10-codexhub.conf"))
                .unwrap()
                .contains(&format!("CODEX_HOME={}", profile_path.display()))
        );

        fs::remove_dir_all(&root).ok();
        std::env::remove_var("CODEXHUB_HOME");
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    #[test]
    fn reads_active_profile_name() {
        let _guard = crate::test_support::env_lock();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("codexhub-active-test-{stamp}"));
        std::env::set_var("CODEXHUB_HOME", &root);

        crate::config::init().unwrap();
        fs::write(root.join("current_profile"), "personal\n").unwrap();

        assert_eq!(active_profile_name().unwrap().as_deref(), Some("personal"));

        fs::remove_dir_all(&root).ok();
        std::env::remove_var("CODEXHUB_HOME");
    }
}
