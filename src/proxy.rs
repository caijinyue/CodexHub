use crate::config::{self, ProxyConfig, ProxyMode};
use anyhow::{Context, Result};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
use std::time::Duration;

const PROXY_VARS: &[&str] = &[
    "http_proxy",
    "HTTP_PROXY",
    "https_proxy",
    "HTTPS_PROXY",
    "all_proxy",
    "ALL_PROXY",
    "no_proxy",
    "NO_PROXY",
];

pub fn apply(command: &mut Command) -> Result<()> {
    let proxy = config::load()?.proxy;
    validate(&proxy)?;
    apply_config(command, &proxy);
    Ok(())
}

pub fn apply_tokio(command: &mut tokio::process::Command) -> Result<()> {
    let proxy = config::load()?.proxy;
    validate(&proxy)?;
    match proxy.mode {
        ProxyMode::Inherit => {}
        ProxyMode::Off => remove_tokio(command),
        ProxyMode::Custom => {
            remove_tokio(command);
            set_tokio(command, &proxy);
        }
    }
    Ok(())
}

fn apply_config(command: &mut Command, proxy: &ProxyConfig) {
    match proxy.mode {
        ProxyMode::Inherit => {}
        ProxyMode::Off => remove(command),
        ProxyMode::Custom => {
            remove(command);
            set(command, proxy);
        }
    }
}

fn remove(command: &mut Command) {
    for name in PROXY_VARS {
        command.env_remove(name);
    }
}

fn remove_tokio(command: &mut tokio::process::Command) {
    for name in PROXY_VARS {
        command.env_remove(name);
    }
}

fn set(command: &mut Command, proxy: &ProxyConfig) {
    set_pair(
        |key, value| {
            command.env(key, value);
        },
        proxy,
    );
}

fn set_tokio(command: &mut tokio::process::Command, proxy: &ProxyConfig) {
    set_pair(
        |key, value| {
            command.env(key, value);
        },
        proxy,
    );
}

fn set_pair(mut set_env: impl FnMut(&str, &str), proxy: &ProxyConfig) {
    for (lower, upper, value) in [
        ("http_proxy", "HTTP_PROXY", proxy.http.as_str()),
        ("https_proxy", "HTTPS_PROXY", proxy.https.as_str()),
        ("all_proxy", "ALL_PROXY", proxy.all.as_str()),
        ("no_proxy", "NO_PROXY", proxy.no_proxy.as_str()),
    ] {
        if !value.trim().is_empty() {
            set_env(lower, value.trim());
            set_env(upper, value.trim());
        }
    }
}

pub fn validate(proxy: &ProxyConfig) -> Result<()> {
    if proxy.mode != ProxyMode::Custom {
        return Ok(());
    }
    if proxy.http.trim().is_empty() && proxy.https.trim().is_empty() && proxy.all.trim().is_empty()
    {
        anyhow::bail!("custom proxy mode requires at least one proxy URL");
    }
    for (label, value) in [
        ("HTTP", proxy.http.as_str()),
        ("HTTPS", proxy.https.as_str()),
        ("ALL", proxy.all.as_str()),
    ] {
        if !value.trim().is_empty() {
            proxy_endpoint(value).with_context(|| format!("Invalid {label} proxy URL"))?;
        }
    }
    Ok(())
}

pub fn test_connection(proxy: &ProxyConfig) -> Result<String> {
    validate(proxy)?;
    if proxy.mode != ProxyMode::Custom {
        anyhow::bail!(
            "proxy mode is {}; select custom to test an endpoint",
            proxy.mode
        );
    }
    let value = [&proxy.https, &proxy.http, &proxy.all]
        .into_iter()
        .find(|value| !value.trim().is_empty())
        .context("No proxy URL configured")?;
    let (host, port) = proxy_endpoint(value)?;
    let addresses = (host.as_str(), port)
        .to_socket_addrs()
        .with_context(|| format!("Resolving proxy host {host}"))?;
    let timeout = Duration::from_secs(3);
    let mut last_error = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, timeout) {
            Ok(_) => return Ok(format!("Connected to {host}:{port}")),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow::anyhow!("Proxy host resolved to no addresses")))
    .with_context(|| format!("Connecting to proxy {host}:{port}"))
}

fn proxy_endpoint(value: &str) -> Result<(String, u16)> {
    let value = value.trim();
    let (scheme, rest) = value
        .split_once("://")
        .context("expected a URL such as http://127.0.0.1:8080")?;
    let default_port = match scheme.to_ascii_lowercase().as_str() {
        "http" => 80,
        "https" => 443,
        "socks5" | "socks5h" => 1080,
        _ => anyhow::bail!("unsupported proxy scheme {scheme}"),
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.is_empty() {
        anyhow::bail!("proxy host is empty");
    }
    if let Some(ipv6) = authority.strip_prefix('[') {
        let end = ipv6.find(']').context("unterminated IPv6 address")?;
        let host = &ipv6[..end];
        let suffix = &ipv6[end + 1..];
        let port = if suffix.is_empty() {
            default_port
        } else {
            suffix
                .strip_prefix(':')
                .context("invalid IPv6 proxy port")?
                .parse()?
        };
        return Ok((host.into(), port));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => (host, port.parse()?),
        _ => (authority, default_port),
    };
    Ok((host.into(), port))
}

pub fn masked(value: &str) -> String {
    let Some((scheme, rest)) = value.split_once("://") else {
        return value.into();
    };
    match rest.rsplit_once('@') {
        Some((_, host)) => format!("{scheme}://***@{host}"),
        None => value.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_proxy_sets_lower_and_upper_case_variables() {
        let mut command = Command::new("codex");
        let proxy = ProxyConfig {
            mode: ProxyMode::Custom,
            http: "http://127.0.0.1:8080".into(),
            https: "http://127.0.0.1:8080".into(),
            all: "socks5://127.0.0.1:1080".into(),
            no_proxy: "localhost".into(),
        };
        apply_config(&mut command, &proxy);
        let env: std::collections::HashMap<_, _> = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert_eq!(env["http_proxy"].as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(env["HTTP_PROXY"].as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(env["ALL_PROXY"].as_deref(), Some("socks5://127.0.0.1:1080"));
    }

    #[test]
    fn off_mode_explicitly_removes_inherited_variables() {
        let mut command = Command::new("codex");
        apply_config(
            &mut command,
            &ProxyConfig {
                mode: ProxyMode::Off,
                ..ProxyConfig::default()
            },
        );
        assert!(command
            .get_envs()
            .any(|(key, value)| key == "http_proxy" && value.is_none()));
    }

    #[test]
    fn parses_and_masks_authenticated_proxy_urls() {
        assert_eq!(
            proxy_endpoint("socks5://user:pass@[::1]:27183").unwrap(),
            ("::1".into(), 27183)
        );
        assert_eq!(
            masked("http://user:pass@proxy:8080"),
            "http://***@proxy:8080"
        );
    }
}
