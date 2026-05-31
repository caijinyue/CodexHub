use anyhow::{Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Current,
    Available,
    NotFastForward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub local_head: String,
    pub remote_head: String,
    pub remote_ref: String,
    pub repo_path: PathBuf,
}

pub fn check_for_update() -> Result<Option<UpdateInfo>> {
    let repo_path = repo_path();
    let local_head = local_build_head(&repo_path)?;
    let remote_ref = update_remote_ref(&repo_path)?;
    let refspec = format!(
        "+refs/heads/{}:refs/remotes/{}/{}",
        remote_ref.branch, remote_ref.remote, remote_ref.branch
    );

    run_git_quiet(
        &repo_path,
        &["fetch", "--quiet", &remote_ref.remote, &refspec],
    )?;

    let remote_head = git_output(&repo_path, &["rev-parse", &remote_ref.local_ref])?;
    let state = classify_heads(
        &local_head,
        &remote_head,
        is_ancestor(&repo_path, &local_head, &remote_head)?,
    );

    Ok((state == UpdateState::Available).then_some(UpdateInfo {
        local_head,
        remote_head,
        remote_ref: remote_ref.display,
        repo_path,
    }))
}

pub fn install_update(repo_path: &Path) -> Result<()> {
    run_git(repo_path, &["pull", "--ff-only"])?;
    run_command(
        cargo_command()
            .args(["install", "--path"])
            .arg(repo_path)
            .arg("--locked")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit()),
    )
    .context("Installing updated codexhub")
}

fn repo_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn local_build_head(repo_path: &Path) -> Result<String> {
    option_env!("CODEXHUB_BUILD_GIT_HEAD")
        .filter(|head| !head.trim().is_empty())
        .map(|head| head.trim().to_string())
        .map(Ok)
        .unwrap_or_else(|| git_output(repo_path, &["rev-parse", "HEAD"]))
}

fn git_output(repo_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .with_context(|| format!("Running git {}", args.join(" ")))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_status(repo_path: &Path, args: &[&str]) -> Result<std::process::ExitStatus> {
    Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .status()
        .with_context(|| format!("Running git {}", args.join(" ")))
}

fn run_git(repo_path: &Path, args: &[&str]) -> Result<()> {
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(args)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit()),
    )
    .with_context(|| format!("Running git {}", args.join(" ")))
}

fn run_git_quiet(repo_path: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .with_context(|| format!("Running git {}", args.join(" ")))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn run_command(command: &mut Command) -> Result<()> {
    let status = command.status().context("Starting command")?;
    if !status.success() {
        anyhow::bail!("Command exited with status {status}");
    }
    Ok(())
}

fn cargo_command() -> Command {
    if let Ok(cargo) = env::var("CARGO") {
        if !cargo.trim().is_empty() {
            return Command::new(cargo);
        }
    }
    if let Some(home) = dirs::home_dir() {
        let rustup_cargo = home.join(".cargo/bin/cargo");
        if rustup_cargo.exists() {
            return Command::new(rustup_cargo);
        }
    }
    Command::new("cargo")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteRef {
    remote: String,
    branch: String,
    display: String,
    local_ref: String,
}

fn update_remote_ref(repo_path: &Path) -> Result<RemoteRef> {
    let upstream = git_output(
        repo_path,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .ok()
    .filter(|value| !value.is_empty());

    let display = match upstream {
        Some(upstream) => upstream,
        None => {
            let branch = git_output(repo_path, &["branch", "--show-current"])?;
            if branch.is_empty() {
                anyhow::bail!("Cannot check updates from a detached HEAD without an upstream");
            }
            format!("origin/{branch}")
        }
    };
    parse_remote_ref(&display).with_context(|| format!("Parsing upstream ref {display}"))
}

fn parse_remote_ref(value: &str) -> Result<RemoteRef> {
    let (remote, branch) = value
        .split_once('/')
        .filter(|(remote, branch)| !remote.is_empty() && !branch.is_empty())
        .context("Expected remote/branch")?;
    Ok(RemoteRef {
        remote: remote.to_string(),
        branch: branch.to_string(),
        display: value.to_string(),
        local_ref: format!("refs/remotes/{remote}/{branch}"),
    })
}

fn is_ancestor(repo_path: &Path, local: &str, remote: &str) -> Result<bool> {
    let status = git_status(repo_path, &["merge-base", "--is-ancestor", local, remote])?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => anyhow::bail!("git merge-base --is-ancestor exited with status {status}"),
    }
}

fn classify_heads(local: &str, remote: &str, local_is_ancestor_of_remote: bool) -> UpdateState {
    if local.trim() == remote.trim() {
        UpdateState::Current
    } else if local_is_ancestor_of_remote {
        UpdateState::Available
    } else {
        UpdateState::NotFastForward
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remote_ref() {
        let remote_ref = parse_remote_ref("origin/main").unwrap();

        assert_eq!(remote_ref.remote, "origin");
        assert_eq!(remote_ref.branch, "main");
        assert_eq!(remote_ref.display, "origin/main");
        assert_eq!(remote_ref.local_ref, "refs/remotes/origin/main");
    }

    #[test]
    fn parses_remote_ref_with_slash_branch() {
        let remote_ref = parse_remote_ref("origin/release/next").unwrap();

        assert_eq!(remote_ref.remote, "origin");
        assert_eq!(remote_ref.branch, "release/next");
        assert_eq!(remote_ref.local_ref, "refs/remotes/origin/release/next");
    }

    #[test]
    fn classifies_update_state() {
        assert_eq!(classify_heads("abc", "abc", false), UpdateState::Current);
        assert_eq!(classify_heads("abc", "def", true), UpdateState::Available);
        assert_eq!(
            classify_heads("abc", "def", false),
            UpdateState::NotFastForward
        );
    }

    #[test]
    fn uses_compiled_git_head_for_local_update_state() {
        assert_eq!(
            local_build_head(Path::new("/does/not/matter")).unwrap(),
            env!("CODEXHUB_BUILD_GIT_HEAD").to_string()
        );
    }
}
