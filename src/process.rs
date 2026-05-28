use crate::profile;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::Write;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus {
    pub plan_type: Option<String>,
    pub primary_remaining_percent: Option<u8>,
    pub secondary_remaining_percent: Option<u8>,
}

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

pub fn codex_account_status(name: &str) -> Result<AccountStatus> {
    let home = profile::ensure_exists(name)?;
    let mut child = Command::new("timeout")
        .args(["12", "codex", "app-server", "--listen", "stdio://"])
        .env("CODEX_HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| "Failed to start official codex app-server")?;

    let mut stdin = child.stdin.take().context("Opening app-server stdin")?;
    writeln!(
        stdin,
        r#"{{"id":1,"method":"initialize","params":{{"clientInfo":{{"name":"codexhub","title":"CodexHub","version":"0.1.0"}},"capabilities":{{"experimentalApi":true,"requestAttestation":false}}}}}}"#
    )?;
    thread::sleep(Duration::from_millis(500));
    writeln!(stdin, r#"{{"id":2,"method":"account/rateLimits/read"}}"#)?;
    thread::sleep(Duration::from_secs(4));
    drop(stdin);

    let out = child
        .wait_with_output()
        .with_context(|| "Reading codex app-server output")?;
    let text = String::from_utf8_lossy(&out.stdout);
    parse_account_status(&text).context("Parsing codex account status")
}

fn parse_account_status(text: &str) -> Option<AccountStatus> {
    for line in text.lines() {
        let value: AppServerLine = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.id == Some(2) {
            let limits = value.result?.rate_limits;
            return Some(AccountStatus {
                plan_type: limits.plan_type,
                primary_remaining_percent: limits
                    .primary
                    .and_then(|w| remaining_percent(w.used_percent)),
                secondary_remaining_percent: limits
                    .secondary
                    .and_then(|w| remaining_percent(w.used_percent)),
            });
        }
    }
    None
}

fn remaining_percent(used_percent: u8) -> Option<u8> {
    Some(100u8.saturating_sub(used_percent.min(100)))
}

#[derive(Debug, Deserialize)]
struct AppServerLine {
    id: Option<u64>,
    result: Option<RateLimitsResponse>,
}

#[derive(Debug, Deserialize)]
struct RateLimitsResponse {
    #[serde(rename = "rateLimits")]
    rate_limits: RateLimitSnapshot,
}

#[derive(Debug, Deserialize)]
struct RateLimitSnapshot {
    #[serde(rename = "planType")]
    plan_type: Option<String>,
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
}

#[derive(Debug, Deserialize)]
struct RateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_account_rate_limit_status() {
        let output = r#"{"id":1,"result":{"codexHome":"/tmp"}}
{"method":"remoteControl/status/changed","params":{"status":"disabled"}}
{"id":2,"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":1,"windowDurationMins":300,"resetsAt":1779973549},"secondary":{"usedPercent":19,"windowDurationMins":10080,"resetsAt":1780484441},"planType":"plus"}}}"#;

        let status = parse_account_status(output).unwrap();

        assert_eq!(status.plan_type.as_deref(), Some("plus"));
        assert_eq!(status.primary_remaining_percent, Some(99));
        assert_eq!(status.secondary_remaining_percent, Some(81));
    }
}
