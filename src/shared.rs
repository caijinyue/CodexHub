use crate::{config, profile};
use anyhow::{Context, Result};
use chrono::Local;
use std::fs;
use std::path::Path;

pub const ALLOWED_SHARED: &[&str] = &[
    "plugins",
    "vendor_imports",
    "skills",
    "rules",
    "models_cache.json",
    "computer-use",
    "cache",
];

#[cfg(unix)]
fn symlink_path(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dst)
}

#[cfg(windows)]
fn symlink_path(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        std::os::windows::fs::symlink_dir(src, dst)
    } else {
        std::os::windows::fs::symlink_file(src, dst)
    }
}

pub fn share_cache(name: &str) -> Result<()> {
    let paths = config::init()?;
    let profile = profile::ensure_exists(name)?;
    let stamp = Local::now().format("%Y%m%d%H%M%S").to_string();

    for item in ALLOWED_SHARED {
        let source = paths.shared.join(item);
        if item.ends_with(".json") {
            if !source.exists() {
                fs::write(&source, b"{}\n")
                    .with_context(|| format!("Creating {}", source.display()))?;
            }
        } else {
            fs::create_dir_all(&source)
                .with_context(|| format!("Creating {}", source.display()))?;
        }

        let target = profile.join(item);
        if fs::symlink_metadata(&target)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            fs::remove_file(&target).with_context(|| format!("Removing {}", target.display()))?;
        } else if target.exists() {
            let backup_name = format!("{name}.{item}.bak.{stamp}").replace('/', "_");
            let backup = paths.backups.join(backup_name);
            fs::rename(&target, &backup).with_context(|| {
                format!("Backing up {} to {}", target.display(), backup.display())
            })?;
        }
        symlink_path(&source, &target)
            .with_context(|| format!("Linking {} -> {}", target.display(), source.display()))?;
    }
    Ok(())
}

pub fn unshare_cache(name: &str, restore_backup: bool, keep_empty: bool) -> Result<()> {
    let paths = config::init()?;
    let profile = profile::ensure_exists(name)?;
    for item in ALLOWED_SHARED {
        let target = profile.join(item);
        if fs::symlink_metadata(&target)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            fs::remove_file(&target).with_context(|| format!("Removing {}", target.display()))?;
            if restore_backup {
                if let Some(backup) = latest_backup(&paths.backups, name, item)? {
                    fs::rename(&backup, &target).with_context(|| {
                        format!("Restoring {} to {}", backup.display(), target.display())
                    })?;
                    continue;
                }
            }
            if keep_empty {
                if item.ends_with(".json") {
                    fs::write(&target, b"{}\n")
                        .with_context(|| format!("Creating {}", target.display()))?;
                } else {
                    fs::create_dir_all(&target)
                        .with_context(|| format!("Creating {}", target.display()))?;
                }
            }
        }
    }
    Ok(())
}

fn latest_backup(dir: &Path, name: &str, item: &str) -> Result<Option<std::path::PathBuf>> {
    let prefix = format!("{name}.{item}.bak.").replace('/', "_");
    let mut matches = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries {
            let entry = entry?;
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with(&prefix) {
                matches.push(entry.path());
            }
        }
    }
    matches.sort();
    Ok(matches.pop())
}
