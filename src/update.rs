use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Current,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub local_head: String,
    pub remote_head: String,
    pub repo_path: PathBuf,
}

pub fn check_for_update() -> Result<Option<UpdateInfo>> {
    let repo_path = repo_path();
    let local_head = git_output(&repo_path, &["rev-parse", "HEAD"])?;
    let remote_output = git_output(&repo_path, &["ls-remote", "origin", "HEAD"])?;
    let remote_head = parse_ls_remote_head(&remote_output).context("Parsing remote HEAD")?;
    Ok(
        (classify_heads(&local_head, &remote_head) == UpdateState::Available).then_some(
            UpdateInfo {
                local_head,
                remote_head,
                repo_path,
            },
        ),
    )
}

pub fn install_update(repo_path: &Path) -> Result<()> {
    run_git(repo_path, &["pull", "--ff-only"])?;
    run_command(
        Command::new("cargo")
            .args(["install", "--path"])
            .arg(repo_path)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit()),
    )
    .context("Installing updated codexhub")
}

fn repo_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

fn run_command(command: &mut Command) -> Result<()> {
    let status = command.status().context("Starting command")?;
    if !status.success() {
        anyhow::bail!("Command exited with status {status}");
    }
    Ok(())
}

fn parse_ls_remote_head(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.split_once(char::is_whitespace).map(|(head, _)| head))
        .filter(|head| !head.is_empty())
        .map(str::to_string)
}

fn classify_heads(local: &str, remote: &str) -> UpdateState {
    if local.trim() == remote.trim() {
        UpdateState::Current
    } else {
        UpdateState::Available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ls_remote_head_line() {
        let line = "70373e921f91f1ae899f0f3a695d313050f460d9\tHEAD\n";

        assert_eq!(
            parse_ls_remote_head(line),
            Some("70373e921f91f1ae899f0f3a695d313050f460d9".to_string())
        );
    }

    #[test]
    fn classifies_update_state() {
        assert_eq!(classify_heads("abc", "abc"), UpdateState::Current);
        assert_eq!(classify_heads("abc", "def"), UpdateState::Available);
    }
}
