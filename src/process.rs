use crate::profile;
use anyhow::{Context, Result};
use std::process::{Command, Stdio};

pub fn codex_login(name: &str) -> Result<i32> {
    run_codex(name, ["login"])
}

pub fn codex_run(name: &str, args: &[String]) -> Result<i32> {
    run_codex(name, args.iter().map(String::as_str))
}

pub fn codex_exec(name: &str, args: &[String]) -> Result<i32> {
    let mut all = vec!["exec".to_string()];
    all.extend(args.iter().cloned());
    run_codex(name, all.iter().map(String::as_str))
}

pub fn run_codex<'a, I>(name: &str, args: I) -> Result<i32>
where
    I: IntoIterator<Item = &'a str>,
{
    let home = profile::ensure_exists(name)?;
    let status = Command::new("codex")
        .args(args)
        .env("CODEX_HOME", &home)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| "Failed to execute official codex CLI")?;
    Ok(status.code().unwrap_or(1))
}
