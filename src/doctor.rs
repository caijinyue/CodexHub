use crate::{config, profile};
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Ok => "OK",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Check {
    pub level: Level,
    pub subject: String,
    pub message: String,
}

pub fn run(allow_auth_symlink: bool) -> Result<Vec<Check>> {
    let mut checks = Vec::new();
    checks.extend(check_codex());
    let profiles = profile::list()?;
    let mut auth_inodes: HashMap<(u64, u64), String> = HashMap::new();
    let mut auth_realpaths: HashMap<PathBuf, String> = HashMap::new();
    let mut sensitive_realpaths: HashMap<PathBuf, (String, String)> = HashMap::new();

    for info in profiles {
        let name = info.name.clone();
        checks.push(ok(&name, "profile directory exists"));
        check_path(
            &mut checks,
            &name,
            &info.path.join("config.toml"),
            "config.toml exists",
            true,
        );
        check_path(
            &mut checks,
            &name,
            &info.path.join("sessions"),
            "sessions exists",
            false,
        );
        check_path(
            &mut checks,
            &name,
            &info.path.join("history.jsonl"),
            "history.jsonl exists",
            true,
        );
        check_path(
            &mut checks,
            &name,
            &info.path.join("session_index.jsonl"),
            "session_index.jsonl exists",
            true,
        );

        let auth = info.path.join("auth.json");
        if auth.exists() {
            checks.push(ok(&name, "auth.json exists"));
            if let Ok(meta) = fs::symlink_metadata(&auth) {
                if meta.file_type().is_symlink() && !allow_auth_symlink {
                    checks.push(error(&name, "auth.json is a symlink"));
                } else if meta.file_type().is_symlink() {
                    checks.push(warn(&name, "auth.json symlink allowed by explicit flag"));
                } else {
                    checks.push(ok(&name, "auth.json is not a symlink"));
                }
            }
            if let Ok(meta) = fs::metadata(&auth) {
                let key = (meta.dev(), meta.ino());
                if let Some(other) = auth_inodes.insert(key, name.clone()) {
                    checks.push(error(
                        "security",
                        format!(
                            "profiles \"{other}\" and \"{name}\" share the same auth.json inode"
                        ),
                    ));
                } else {
                    checks.push(ok(&name, format!("auth.json inode {}", meta.ino())));
                }
            }
            if let Ok(real) = fs::canonicalize(&auth) {
                if let Some(other) = auth_realpaths.insert(real.clone(), name.clone()) {
                    checks.push(error(
                        "security",
                        format!(
                            "profiles \"{other}\" and \"{name}\" share auth.json real path {}",
                            real.display()
                        ),
                    ));
                }
            }
        } else {
            checks.push(warn(&name, "auth.json missing; run codexhub login <name>"));
        }

        for entry in WalkDir::new(&info.path)
            .follow_links(false)
            .into_iter()
            .flatten()
        {
            let p = entry.path();
            if config::is_broken_symlink(p) {
                checks.push(error(&name, format!("broken symlink {}", p.display())));
            }
        }

        for rel in sensitive_paths(&info.path) {
            if let Ok(real) = fs::canonicalize(&rel.1) {
                if let Some((other_profile, other_label)) =
                    sensitive_realpaths.insert(real.clone(), (name.clone(), rel.0.clone()))
                {
                    checks.push(error(
                        "security",
                        format!(
                            "{name}:{} shares real path with {other_profile}:{} ({})",
                            rel.0,
                            other_label,
                            real.display()
                        ),
                    ));
                }
            }
            if fs::symlink_metadata(&rel.1)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                checks.push(error(
                    &name,
                    format!("sensitive path {} is a symlink", rel.0),
                ));
            }
        }
    }
    Ok(checks)
}

fn check_codex() -> Vec<Check> {
    let mut checks = Vec::new();
    match Command::new("codex").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
            checks.push(ok("codex", "binary found"));
            checks.push(ok("codex", format!("version {version}")));
            checks.push(ok(
                "codex",
                "CODEX_HOME is passed by CodexHub process launcher",
            ));
        }
        Ok(out) => checks.push(error(
            "codex",
            format!("codex --version failed with status {}", out.status),
        )),
        Err(err) => checks.push(error("codex", format!("binary not available: {err}"))),
    }
    checks
}

fn check_path(checks: &mut Vec<Check>, subject: &str, path: &Path, msg: &str, warn_only: bool) {
    if path.exists() {
        checks.push(ok(subject, msg));
    } else if warn_only {
        checks.push(warn(subject, format!("{msg}: missing")));
    } else {
        checks.push(error(subject, format!("{msg}: missing")));
    }
}

fn sensitive_paths(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = vec![
        ("sessions".to_string(), root.join("sessions")),
        ("history.jsonl".to_string(), root.join("history.jsonl")),
        (
            "session_index.jsonl".to_string(),
            root.join("session_index.jsonl"),
        ),
        ("installation_id".to_string(), root.join("installation_id")),
        (
            ".credentials.json".to_string(),
            root.join(".credentials.json"),
        ),
    ];
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if (name.starts_with("state_")
                || name.starts_with("goals_")
                || name.starts_with("logs_"))
                && name.ends_with(".sqlite")
            {
                out.push((name, entry.path()));
            }
        }
    }
    out
}

fn ok(subject: impl Into<String>, message: impl Into<String>) -> Check {
    Check {
        level: Level::Ok,
        subject: subject.into(),
        message: message.into(),
    }
}

fn warn(subject: impl Into<String>, message: impl Into<String>) -> Check {
    Check {
        level: Level::Warn,
        subject: subject.into(),
        message: message.into(),
    }
}

fn error(subject: impl Into<String>, message: impl Into<String>) -> Check {
    Check {
        level: Level::Error,
        subject: subject.into(),
        message: message.into(),
    }
}
