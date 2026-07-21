use crate::{config, process, profile, size};
use anyhow::{Context, Result};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::Mutex,
};
use uuid::Uuid;

const SESSION_COOKIE: &str = "codexhub_remote";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemoteConfig {
    password_hash: String,
}

#[derive(Debug, Clone)]
pub struct RemoteStatus {
    pub config_path: PathBuf,
    pub password_configured: bool,
    pub localhost_url: String,
    pub tailscale_url: String,
    pub localhost_command: String,
    pub tailscale_command: String,
}

#[derive(Clone)]
struct RemoteState {
    auth: Arc<Mutex<HashSet<String>>>,
    sessions: Arc<Mutex<HashMap<Uuid, SessionRecord>>>,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    view: RemoteSession,
    events: Vec<SessionEvent>,
}

#[derive(Debug, Clone, Serialize)]
struct RemoteSession {
    id: Uuid,
    profile: String,
    cwd: String,
    prompt: String,
    status: SessionStatus,
    started_at: DateTime<Local>,
    exit_code: Option<i32>,
    pid: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SessionStatus {
    Running,
    Exited,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
struct SessionEvent {
    stream: String,
    text: String,
    timestamp: DateTime<Local>,
}

#[derive(Debug, Serialize)]
struct ProfileResponse {
    name: String,
    logged_in: bool,
    active: bool,
    path: String,
    plan_type: Option<String>,
    limit_5h_label: String,
    limit_5h_remaining: Option<u8>,
    limit_7day_label: String,
    limit_7day_remaining: Option<u8>,
    plan_expires_at: Option<String>,
    used_since: Option<String>,
    sessions_size: String,
    logs_size: String,
    total_size: String,
    shared_cache: bool,
}

#[derive(Debug, Serialize)]
struct HistoryResponse {
    profile: String,
    session_id: String,
    title: String,
    cwd: Option<String>,
    path: Option<String>,
    updated_at: i64,
    preview: Vec<PreviewResponse>,
}

#[derive(Debug, Serialize)]
struct PreviewResponse {
    role: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct AuthResponse {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct InfoResponse {
    cwd: String,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    password: String,
}

#[derive(Debug, Deserialize)]
struct StartSessionRequest {
    profile: String,
    prompt: String,
    cwd: Option<String>,
}

#[derive(Debug, Serialize)]
struct StartSessionResponse {
    id: Uuid,
}

pub fn set_password(password: &str) -> Result<()> {
    if password.len() < 8 {
        anyhow::bail!("Remote password must be at least 8 characters");
    }
    let remote = RemoteConfig {
        password_hash: hash_password(password)?,
    };
    write_config(&remote)
}

pub fn status() -> Result<RemoteStatus> {
    let config_path = config_path()?;
    Ok(RemoteStatus {
        config_path: config_path.clone(),
        password_configured: config_path.exists()
            && read_config()
                .map(|config| !config.password_hash.trim().is_empty())
                .unwrap_or(false),
        localhost_url: "http://127.0.0.1:17777".into(),
        tailscale_url: "http://<tailscale-ip>:17777".into(),
        localhost_command: "codexhub serve --host 127.0.0.1 --port 17777".into(),
        tailscale_command: "codexhub serve --host 0.0.0.0 --port 17777".into(),
    })
}

pub fn run_server(host: String, port: u16, password: Option<String>) -> Result<()> {
    let config = ensure_config(password)?;
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .with_context(|| format!("Parsing listen address {host}:{port}"))?;
    let runtime = tokio::runtime::Runtime::new().context("Starting tokio runtime")?;
    runtime.block_on(async move {
        let state = RemoteState {
            auth: Arc::new(Mutex::new(HashSet::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        };
        let app = Router::new()
            .route("/", get(index))
            .route("/api/login", post(login))
            .route("/api/me", get(me))
            .route("/api/info", get(info))
            .route("/api/profiles", get(profiles))
            .route("/api/profiles/:name/activate", post(activate_profile))
            .route("/api/history", get(history))
            .route("/api/sessions", get(list_sessions).post(start_session))
            .route("/api/sessions/:id/stop", post(stop_session))
            .route("/ws/sessions/:id", get(session_ws))
            .with_state((state, config));
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("Binding {addr}"))?;
        println!("CodexHub Remote listening on http://{addr}");
        println!("Use Tailscale or an SSH tunnel before exposing this outside localhost.");
        axum::serve(listener, app)
            .await
            .context("Serving remote UI")
    })
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn login(
    State((state, config)): State<(RemoteState, RemoteConfig)>,
    Json(request): Json<LoginRequest>,
) -> Response {
    if !verify_password(&config.password_hash, &request.password) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let token = Uuid::new_v4().to_string();
    state.auth.lock().await.insert(token.clone());
    let mut response = Json(AuthResponse { ok: true }).into_response();
    let cookie = format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Lax; Path=/");
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).unwrap_or_else(|_| HeaderValue::from_static("")),
    );
    response
}

async fn me(State((state, _)): State<(RemoteState, RemoteConfig)>, headers: HeaderMap) -> Response {
    if authenticated(&state, &headers).await {
        Json(AuthResponse { ok: true }).into_response()
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

async fn info(
    State((state, _)): State<(RemoteState, RemoteConfig)>,
    headers: HeaderMap,
) -> Response {
    if !authenticated(&state, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let cwd = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .display()
        .to_string();
    Json(InfoResponse { cwd }).into_response()
}

async fn profiles(
    State((state, _)): State<(RemoteState, RemoteConfig)>,
    headers: HeaderMap,
) -> Response {
    if !authenticated(&state, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match profile::list().and_then(|profiles| {
        let active = crate::activation::active_profile_name()?;
        Ok(profiles
            .into_iter()
            .map(|profile| profile_response(profile, active.as_deref()))
            .collect::<Vec<_>>())
    }) {
        Ok(profiles) => Json(profiles).into_response(),
        Err(err) => error_response(err),
    }
}

async fn activate_profile(
    State((state, _)): State<(RemoteState, RemoteConfig)>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    if !authenticated(&state, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match crate::activation::activate_profile(&name) {
        Ok(_) => Json(AuthResponse { ok: true }).into_response(),
        Err(err) => error_response(err),
    }
}

async fn history(
    State((state, _)): State<(RemoteState, RemoteConfig)>,
    headers: HeaderMap,
) -> Response {
    if !authenticated(&state, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match load_history() {
        Ok(history) => Json(history).into_response(),
        Err(err) => error_response(err),
    }
}

async fn list_sessions(
    State((state, _)): State<(RemoteState, RemoteConfig)>,
    headers: HeaderMap,
) -> Response {
    if !authenticated(&state, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mut sessions = state
        .sessions
        .lock()
        .await
        .values()
        .map(|record| record.view.clone())
        .collect::<Vec<_>>();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.started_at));
    Json(sessions).into_response()
}

async fn start_session(
    State((state, _)): State<(RemoteState, RemoteConfig)>,
    headers: HeaderMap,
    Json(request): Json<StartSessionRequest>,
) -> Response {
    if !authenticated(&state, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match spawn_session(state.clone(), request).await {
        Ok(id) => Json(StartSessionResponse { id }).into_response(),
        Err(err) => error_response(err),
    }
}

async fn stop_session(
    State((state, _)): State<(RemoteState, RemoteConfig)>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    if !authenticated(&state, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let pid = {
        let sessions = state.sessions.lock().await;
        sessions.get(&id).and_then(|record| record.view.pid)
    };
    let Some(pid) = pid else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Err(err) = stop_pid(pid).await {
        return error_response(err);
    }
    let mut sessions = state.sessions.lock().await;
    if let Some(record) = sessions.get_mut(&id) {
        record.view.status = SessionStatus::Stopped;
    }
    Json(AuthResponse { ok: true }).into_response()
}

async fn session_ws(
    State((state, _)): State<(RemoteState, RemoteConfig)>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> Response {
    if !authenticated(&state, &headers).await {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.on_upgrade(move |socket| stream_session(socket, state, id))
}

async fn stream_session(mut socket: WebSocket, state: RemoteState, id: Uuid) {
    let mut next = 0usize;
    loop {
        let (view, events) = {
            let sessions = state.sessions.lock().await;
            let Some(record) = sessions.get(&id) else {
                let _ = socket
                    .send(Message::Text(r#"{"type":"missing"}"#.into()))
                    .await;
                return;
            };
            (record.view.clone(), record.events[next..].to_vec())
        };
        next += events.len();
        let payload = serde_json::json!({
            "type": "session",
            "session": view,
            "events": events,
        });
        if socket
            .send(Message::Text(payload.to_string()))
            .await
            .is_err()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn stop_pid(pid: u32) -> Result<()> {
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("taskkill");
        command.args(["/PID", &pid.to_string(), "/T", "/F"]);
        command
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut command = Command::new("kill");
        command.arg(pid.to_string());
        command
    };
    let status = command.status().await.context("Stopping remote session")?;
    if !status.success() {
        anyhow::bail!("Stop command exited with status {status}");
    }
    Ok(())
}

async fn spawn_session(state: RemoteState, request: StartSessionRequest) -> Result<Uuid> {
    config::ensure_profile_name(&request.profile)?;
    if request.prompt.trim().is_empty() {
        anyhow::bail!("Prompt is required");
    }
    let codex_home = profile::ensure_exists(&request.profile)?;
    let cwd = request
        .cwd
        .filter(|cwd| !cwd.trim().is_empty())
        .unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .display()
                .to_string()
        });
    let mut command = Command::new("codex");
    command
        .arg("exec")
        .arg(&request.prompt)
        .env("CODEX_HOME", codex_home)
        .current_dir(config::expand_tilde(PathBuf::from(&cwd)))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    crate::proxy::apply_tokio(&mut command)?;
    let mut child = command.spawn().context("Starting codex exec")?;
    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let id = Uuid::new_v4();
    let record = SessionRecord {
        view: RemoteSession {
            id,
            profile: request.profile,
            cwd,
            prompt: request.prompt,
            status: SessionStatus::Running,
            started_at: Local::now(),
            exit_code: None,
            pid,
        },
        events: Vec::new(),
    };
    let mut record = record;
    record.events.push(SessionEvent {
        stream: "system".into(),
        text: "session started".into(),
        timestamp: Local::now(),
    });
    state.sessions.lock().await.insert(id, record);
    if let Some(stdout) = stdout {
        tokio::spawn(read_stream(state.clone(), id, "stdout", stdout));
    }
    if let Some(stderr) = stderr {
        tokio::spawn(read_stream(state.clone(), id, "stderr", stderr));
    }
    tokio::spawn(async move {
        let result = child.wait().await;
        let mut sessions = state.sessions.lock().await;
        if let Some(record) = sessions.get_mut(&id) {
            match result {
                Ok(status) => {
                    record.view.exit_code = status.code();
                    if record.view.status != SessionStatus::Stopped {
                        record.view.status = SessionStatus::Exited;
                    }
                }
                Err(err) => {
                    record.view.status = SessionStatus::Failed;
                    record.events.push(SessionEvent {
                        stream: "system".into(),
                        text: err.to_string(),
                        timestamp: Local::now(),
                    });
                }
            }
        }
    });
    Ok(id)
}

async fn read_stream<R>(state: RemoteState, id: Uuid, stream: &'static str, reader: R)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut sessions = state.sessions.lock().await;
        if let Some(record) = sessions.get_mut(&id) {
            record.events.push(SessionEvent {
                stream: stream.into(),
                text: line,
                timestamp: Local::now(),
            });
        }
    }
}

async fn authenticated(state: &RemoteState, headers: &HeaderMap) -> bool {
    let Some(cookie) = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(token) = cookie.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix(&format!("{SESSION_COOKIE}="))
    }) else {
        return false;
    };
    state.auth.lock().await.contains(token)
}

fn load_history() -> Result<Vec<HistoryResponse>> {
    let mut out = Vec::new();
    for profile in profile::list()? {
        if !profile.logged_in {
            continue;
        }
        for session in process::codex_history_sessions(&profile.name, 100).unwrap_or_default() {
            let preview = process::session_preview_messages(&session, 12)
                .into_iter()
                .filter(|message| !message.text.is_empty())
                .map(|message| PreviewResponse {
                    role: match message.role {
                        process::PreviewRole::User => "user".into(),
                        process::PreviewRole::Assistant => "assistant".into(),
                    },
                    text: message.text,
                })
                .collect();
            out.push(HistoryResponse {
                profile: session.profile,
                session_id: session.session_id,
                title: session.title,
                cwd: session.cwd,
                path: session.path,
                updated_at: session.updated_at,
                preview,
            });
        }
    }
    out.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    Ok(out)
}

fn profile_response(profile: profile::ProfileInfo, active: Option<&str>) -> ProfileResponse {
    ProfileResponse {
        active: active == Some(profile.name.as_str()),
        name: profile.name,
        logged_in: profile.logged_in,
        path: profile.path.display().to_string(),
        plan_type: profile.plan_type,
        limit_5h_label: profile.limit_5h_label,
        limit_5h_remaining: profile.limit_5h_remaining,
        limit_7day_label: profile.limit_7day_label,
        limit_7day_remaining: profile.limit_7day_remaining,
        plan_expires_at: date(profile.plan_expires_at),
        used_since: date(profile.used_since),
        sessions_size: size::human(profile.sessions_size),
        logs_size: size::human(profile.logs_size),
        total_size: size::human(profile.total_size),
        shared_cache: profile.shared_cache,
    }
}

fn date(value: Option<DateTime<Local>>) -> Option<String> {
    value.map(|value| value.format("%Y-%m-%d").to_string())
}

fn ensure_config(password: Option<String>) -> Result<RemoteConfig> {
    let path = config_path()?;
    if let Some(password) = password {
        set_password(&password)?;
    }
    if path.exists() {
        return read_config();
    }
    let initial = Uuid::new_v4().to_string();
    let remote = RemoteConfig {
        password_hash: hash_password(&initial)?,
    };
    write_config(&remote)?;
    println!("Generated initial CodexHub Remote password: {initial}");
    println!("Change it with: codexhub remote password <new-password>");
    Ok(remote)
}

fn read_config() -> Result<RemoteConfig> {
    let path = config_path()?;
    let data = fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
    toml::from_str(&data).with_context(|| format!("Parsing {}", path.display()))
}

fn write_config(remote: &RemoteConfig) -> Result<()> {
    let path = config_path()?;
    let Some(parent) = path.parent() else {
        anyhow::bail!("Invalid remote config path");
    };
    fs::create_dir_all(parent)?;
    fs::write(&path, toml::to_string_pretty(remote)?)
        .with_context(|| format!("Writing {}", path.display()))
}

fn config_path() -> Result<PathBuf> {
    Ok(config::init()?.root.join("remote.toml"))
}

fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|err| anyhow::anyhow!("{err}"))?
        .to_string())
}

fn verify_password(hash: &str, password: &str) -> bool {
    let Ok(hash) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .is_ok()
}

fn error_response(err: anyhow::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}

#[allow(dead_code)]
fn path_exists(path: &FsPath) -> bool {
    path.exists()
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>CodexHub Remote</title>
  <style>
    :root { color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    body { margin: 0; background: #0f172a; color: #e2e8f0; }
    button, input, textarea, select { font: inherit; }
    .app { min-height: 100vh; display: grid; grid-template-columns: 320px 1fr; }
    .side { border-right: 1px solid #334155; background: #111827; padding: 18px; overflow: auto; }
    .main { padding: 18px; overflow: auto; }
    .top { display: flex; justify-content: space-between; gap: 12px; align-items: center; margin-bottom: 12px; }
    .status { position: sticky; top: 0; z-index: 5; background: #172033; border: 1px solid #334155; border-radius: 8px; padding: 8px 10px; margin-bottom: 12px; }
    h1 { margin: 0 0 14px; font-size: 21px; }
    h2 { margin: 20px 0 10px; font-size: 16px; color: #7dd3fc; }
    .card { border: 1px solid #334155; background: #172033; border-radius: 8px; padding: 12px; margin-bottom: 10px; }
    .compact { padding: 8px 10px; }
    .row { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
    .muted { color: #94a3b8; }
    .ok { color: #4ade80; }
    .warn { color: #fbbf24; }
    .danger { color: #fb7185; }
    input, textarea, select { width: 100%; box-sizing: border-box; border: 1px solid #475569; border-radius: 6px; padding: 9px; background: #0b1220; color: #e2e8f0; }
    input[type="checkbox"] { width: auto; }
    textarea { min-height: 120px; resize: vertical; }
    button { border: 0; border-radius: 6px; padding: 9px 12px; background: #38bdf8; color: #020617; font-weight: 700; cursor: pointer; }
    button:disabled { opacity: .55; cursor: not-allowed; }
    button.secondary { background: #334155; color: #e2e8f0; }
    button.danger { background: #fb7185; color: #020617; }
    .profile { cursor: pointer; }
    .profile.active { border-color: #38bdf8; }
    .profile.selected { outline: 2px solid #7dd3fc; }
    .log { background: #020617; border: 1px solid #334155; border-radius: 8px; padding: 12px; min-height: 240px; white-space: pre-wrap; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; }
    .prompt { background: #334155; border-radius: 7px; padding: 8px 10px; margin: 8px 0; }
    .answer { margin: 8px 0 14px; color: #dbeafe; }
    .session { cursor: pointer; }
    .session.running { border-color: #4ade80; }
    .pill { border-radius: 999px; padding: 3px 8px; background: #334155; color: #cbd5e1; font-size: 12px; }
    .hidden { display: none; }
    @media (max-width: 820px) { .app { display: block; } .side { border-right: 0; border-bottom: 1px solid #334155; } }
  </style>
</head>
<body>
  <div id="login" class="main hidden">
    <h1>CodexHub Remote</h1>
    <div class="card">
      <h2>Login</h2>
      <input id="password" type="password" placeholder="Password" />
      <p><button id="loginButton">Login</button></p>
      <p id="loginError" class="danger"></p>
    </div>
  </div>
  <div id="app" class="app hidden">
    <aside class="side">
      <div class="top">
        <h1>CodexHub</h1>
        <button id="refreshButton" class="secondary">Refresh</button>
      </div>
      <div id="status" class="status muted">Ready</div>
      <div class="card compact">
        <div class="muted">Server cwd</div>
        <div id="serverCwd">-</div>
      </div>
      <h2>Accounts</h2>
      <div id="profiles"></div>
      <h2>Start Session</h2>
      <select id="profileSelect"></select>
      <p><input id="cwd" placeholder="Working directory" /></p>
      <p><textarea id="prompt" placeholder="Prompt"></textarea></p>
      <div class="row">
        <button id="startButton">Start</button>
        <span id="startError" class="danger"></span>
      </div>
    </aside>
    <main class="main">
      <div class="top">
        <h2>Live Session</h2>
        <span id="sessionStatus" class="muted">No session</span>
      </div>
      <div class="row">
        <button id="stopButton" class="danger">Stop</button>
        <button id="refreshSessionsButton" class="secondary">Refresh Sessions</button>
      </div>
      <div id="sessions"></div>
      <div id="log" class="log"></div>
      <div class="top">
        <h2>History</h2>
        <span id="historyCount" class="muted"></span>
      </div>
      <div class="row">
        <input id="historySearch" placeholder="Search title, account, cwd" />
        <label class="muted"><input id="currentPathOnly" type="checkbox" /> current cwd only</label>
      </div>
      <div id="history"></div>
    </main>
  </div>
<script>
let currentSession = null;
let socket = null;
let profilesCache = [];
let historyCache = [];
let serverInfo = { cwd: '' };
let lastSessionStatus = null;

function setStatus(text, kind = 'muted') {
  const el = document.getElementById('status');
  if (!el) return;
  el.className = `status ${kind}`;
  el.textContent = text;
}

async function api(path, options = {}) {
  const res = await fetch(path, { credentials: 'same-origin', ...options });
  if (!res.ok) {
    let text = await res.text();
    try { text = JSON.parse(text).error || text; } catch {}
    throw new Error(text);
  }
  return res.json();
}

function quotaPart(label, value) {
  return value == null ? '' : `${label || 'quota'} ${value}%`;
}

function quotaText(profile) {
  const parts = [
    quotaPart(profile.limit_5h_label, profile.limit_5h_remaining),
    quotaPart(profile.limit_7day_label, profile.limit_7day_remaining)
  ].filter(Boolean);
  return parts.length ? parts.join(' · ') : 'quota -';
}

async function boot() {
  bindEvents();
  try {
    await api('/api/me');
    showApp();
  } catch {
    document.getElementById('login').classList.remove('hidden');
  }
}

function bindEvents() {
  document.getElementById('loginButton')?.addEventListener('click', login);
  document.getElementById('password')?.addEventListener('keydown', event => {
    if (event.key === 'Enter') login();
  });
  document.getElementById('refreshButton')?.addEventListener('click', () => refreshAll().catch(showError));
  document.getElementById('startButton')?.addEventListener('click', () => startSession().catch(showError));
  document.getElementById('stopButton')?.addEventListener('click', () => stopSession().catch(showError));
  document.getElementById('refreshSessionsButton')?.addEventListener('click', () => loadSessions().catch(showError));
  document.getElementById('historySearch')?.addEventListener('input', renderHistory);
  document.getElementById('currentPathOnly')?.addEventListener('change', renderHistory);
}

function showError(err) {
  setStatus(err?.message || String(err), 'danger');
}

async function login() {
  document.getElementById('loginError').textContent = '';
  try {
    await api('/api/login', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ password: document.getElementById('password').value })
    });
    showApp();
  } catch {
    document.getElementById('loginError').textContent = 'Invalid password';
  }
}

async function showApp() {
  document.getElementById('login').classList.add('hidden');
  document.getElementById('app').classList.remove('hidden');
  await refreshAll();
}

async function refreshAll() {
  setStatus('Refreshing...', 'muted');
  serverInfo = await api('/api/info');
  document.getElementById('serverCwd').textContent = serverInfo.cwd;
  if (!document.getElementById('cwd').value) document.getElementById('cwd').value = serverInfo.cwd;
  const profiles = await api('/api/profiles');
  profilesCache = profiles;
  const profileList = document.getElementById('profiles');
  const select = document.getElementById('profileSelect');
  profileList.innerHTML = '';
  select.innerHTML = '';
  for (const p of profiles) {
    const el = document.createElement('div');
    el.className = 'card profile' + (p.active ? ' active' : '');
    el.onclick = () => selectProfile(p.name);
    el.innerHTML = `<b>${escapeHtml(p.name)}</b> ${p.active ? '<span class="ok">active</span>' : ''}<br>
      <span class="muted">${p.logged_in ? 'signed in' : 'relogin needed'} · ${quotaText(p)}</span><br>
      <span class="muted">member ${p.plan_expires_at ?? '-'} · total ${escapeHtml(p.total_size)}</span>`;
    const actions = document.createElement('p');
    const activate = document.createElement('button');
    activate.className = 'secondary';
    activate.textContent = 'Activate';
    activate.addEventListener('click', event => {
      event.stopPropagation();
      activateProfile(p.name).catch(showError);
    });
    actions.appendChild(activate);
    el.appendChild(actions);
    profileList.appendChild(el);
    const opt = document.createElement('option');
    opt.value = p.name;
    opt.textContent = p.name;
    select.appendChild(opt);
  }
  const active = profiles.find(p => p.active) || profiles[0];
  if (active) selectProfile(active.name);
  const history = await api('/api/history');
  historyCache = history;
  renderHistory();
  await loadSessions();
  setStatus('Ready', 'ok');
}

function selectProfile(name) {
  document.getElementById('profileSelect').value = name;
  for (const el of document.querySelectorAll('.profile')) el.classList.remove('selected');
  const idx = profilesCache.findIndex(p => p.name === name);
  const el = document.querySelectorAll('.profile')[idx];
  if (el) el.classList.add('selected');
}

async function activateProfile(name) {
  setStatus(`Activating ${name}...`, 'muted');
  await api(`/api/profiles/${encodeURIComponent(name)}/activate`, { method: 'POST' });
  await refreshAll();
  setStatus(`Activated ${name}`, 'ok');
}

function renderHistory() {
  const query = document.getElementById('historySearch').value.trim().toLowerCase();
  const currentOnly = document.getElementById('currentPathOnly').checked;
  const filtered = historyCache.filter(h => {
    if (currentOnly && h.cwd !== serverInfo.cwd) return false;
    const haystack = `${h.title} ${h.profile} ${h.cwd || ''}`.toLowerCase();
    return !query || haystack.includes(query);
  });
  const historyEl = document.getElementById('history');
  document.getElementById('historyCount').textContent = `${filtered.length} sessions`;
  historyEl.innerHTML = '';
  for (const h of filtered.slice(0, 80)) {
    const el = document.createElement('div');
    el.className = 'card';
    el.innerHTML = `<b>${escapeHtml(h.title)}</b><br><span class="muted">${escapeHtml(h.profile)} · ${escapeHtml(h.cwd || '-')}</span>`;
    for (const msg of h.preview.slice(0, 4)) {
      const m = document.createElement('div');
      m.className = msg.role === 'user' ? 'prompt' : 'answer';
      m.textContent = msg.text;
      el.appendChild(m);
    }
    historyEl.appendChild(el);
  }
}

async function startSession() {
  const button = document.getElementById('startButton');
  const error = document.getElementById('startError');
  error.textContent = '';
  button.disabled = true;
  setStatus('Starting session...', 'muted');
  const body = {
    profile: document.getElementById('profileSelect').value,
    cwd: document.getElementById('cwd').value,
    prompt: document.getElementById('prompt').value
  };
  try {
    const res = await api('/api/sessions', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body)
    });
    currentSession = res.id;
    document.getElementById('log').textContent = '';
    connectSession(res.id);
    await loadSessions();
    setStatus('Session started', 'ok');
  } catch (err) {
    error.textContent = err.message;
    setStatus(err.message, 'danger');
  } finally {
    button.disabled = false;
  }
}

async function loadSessions() {
  setStatus('Loading sessions...', 'muted');
  const sessions = await api('/api/sessions');
  const target = document.getElementById('sessions');
  target.innerHTML = '';
  if (!sessions.length) {
    setStatus('Ready', 'ok');
    return;
  }
  for (const s of sessions.slice(0, 8)) {
    const el = document.createElement('div');
    el.className = 'card compact session ' + (s.status === 'running' ? 'running' : '');
    el.onclick = () => {
      currentSession = s.id;
      document.getElementById('log').textContent = '';
      connectSession(s.id);
    };
    el.innerHTML = `<b>${escapeHtml(s.profile)}</b> <span class="pill">${escapeHtml(s.status)}</span><br>
      <span class="muted">${escapeHtml(s.prompt.slice(0, 120))}</span>`;
    target.appendChild(el);
  }
  setStatus('Ready', 'ok');
}

function connectSession(id) {
  if (socket) socket.close();
  lastSessionStatus = null;
  setStatus('Connecting live session...', 'muted');
  socket = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws/sessions/${id}`);
  socket.onopen = () => setStatus('Live session connected', 'ok');
  socket.onmessage = event => {
    const data = JSON.parse(event.data);
    if (data.type !== 'session') return;
    document.getElementById('sessionStatus').textContent = `${data.session.status} · ${data.session.profile}`;
    if (data.session.status !== lastSessionStatus) {
      lastSessionStatus = data.session.status;
      loadSessions().catch(() => {});
    }
    const log = document.getElementById('log');
    for (const item of data.events) {
      log.textContent += `[${item.stream}] ${item.text}\n`;
    }
    log.scrollTop = log.scrollHeight;
  };
  socket.onclose = () => {
    document.getElementById('sessionStatus').textContent += ' · disconnected';
    setStatus('Live session disconnected', 'warn');
  };
  socket.onerror = () => {
    setStatus('Live session websocket error', 'danger');
  };
}

async function stopSession() {
  if (!currentSession) return;
  try {
    await api(`/api/sessions/${currentSession}/stop`, { method: 'POST' });
    await loadSessions();
    setStatus('Stop requested', 'ok');
  } catch (err) {
    document.getElementById('sessionStatus').textContent = `stop failed: ${err.message}`;
    setStatus(err.message, 'danger');
  }
}

function escapeHtml(text) {
  return String(text).replace(/[&<>"']/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
}

boot();
</script>
</body>
</html>"#;
