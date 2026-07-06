use crate::{profile, proxy};
use anyhow::{Context, Result};
use std::env;
use std::process::{Command, Stdio};

pub fn open(name: &str) -> Result<i32> {
    let home = profile::ensure_exists(name)?;
    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let prompt = format!("(codex:{name}) $PS1");
    let mut command = Command::new(shell);
    command
        .env("CODEX_HOME", home)
        .env("PS1", prompt)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    proxy::apply(&mut command)?;
    let status = command.status().context("Failed to open profile shell")?;
    Ok(status.code().unwrap_or(1))
}
