use super::{
    screens::{InputMode, Screen},
    widgets::{Theme, ThemePreference},
};
use crate::{doctor, process, profile, update};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

pub struct App {
    pub screen: Screen,
    pub input_mode: InputMode,
    pub profiles: Vec<profile::ProfileInfo>,
    pub active_profile: Option<String>,
    pub selected: usize,
    pub input: String,
    pub message: String,
    pub doctor_checks: Vec<doctor::Check>,
    all_history_sessions: Vec<process::HistorySession>,
    pub history_sessions: Vec<process::HistorySession>,
    pub history_all_paths: bool,
    pub history_cwd: Option<PathBuf>,
    pub selected_history: usize,
    pub selected_continue_profile: usize,
    pub shared_accounts: Vec<crate::shared_account::SharedAccountInfo>,
    pub selected_shared_account: usize,
    pub pending_share_plan: Option<crate::shared_account::ShareAccountPlan>,
    pub pending_login_method: Option<process::LoginMethod>,
    pub update_info: Option<update::UpdateInfo>,
    pub update_error: Option<String>,
    pub status_loading: bool,
    pub history_loading: bool,
    pub update_checking: bool,
    pub quota_refresh_secs: u64,
    pub proxy: crate::config::ProxyConfig,
    last_status_refresh: Option<Instant>,
    pub theme: Theme,
    pub theme_preference: ThemePreference,
    status_rx: Option<Receiver<Vec<(String, process::AccountStatus)>>>,
    history_rx: Option<Receiver<Vec<process::HistorySession>>>,
    update_rx: Option<Receiver<Result<Option<update::UpdateInfo>, String>>>,
}

impl App {
    pub fn new() -> Result<Self> {
        crate::config::init()?;
        let config = crate::config::load()?;
        let profiles = profile::list()?;
        let active_profile = crate::activation::active_profile_name()?;
        let theme_preference = load_theme_preference();
        let mut app = Self {
            screen: Screen::List,
            input_mode: InputMode::None,
            profiles,
            active_profile,
            selected: 0,
            input: String::new(),
            message: String::new(),
            doctor_checks: Vec::new(),
            all_history_sessions: Vec::new(),
            history_sessions: Vec::new(),
            history_all_paths: false,
            history_cwd: std::env::current_dir().ok(),
            selected_history: 0,
            selected_continue_profile: 0,
            shared_accounts: Vec::new(),
            selected_shared_account: 0,
            pending_share_plan: None,
            pending_login_method: None,
            update_info: None,
            update_error: None,
            status_loading: false,
            history_loading: false,
            update_checking: false,
            quota_refresh_secs: config.quota_refresh_secs,
            proxy: config.proxy,
            last_status_refresh: None,
            theme: Theme::from_preference(theme_preference),
            theme_preference,
            status_rx: None,
            history_rx: None,
            update_rx: None,
        };
        app.start_status_refresh();
        app.start_update_check();
        Ok(app)
    }

    pub fn refresh_profiles(&mut self) -> Result<()> {
        let previous = self.profiles.clone();
        self.profiles = profile::list()?;
        preserve_account_status(&mut self.profiles, &previous);
        self.active_profile = crate::activation::active_profile_name()?;
        if self.selected >= self.profiles.len() {
            self.selected = self.profiles.len().saturating_sub(1);
        }
        self.start_status_refresh();
        self.start_update_check();
        Ok(())
    }

    pub fn refresh_profiles_now(&mut self) -> Result<()> {
        let previous = self.profiles.clone();
        self.profiles = profile::list()?;
        preserve_account_status(&mut self.profiles, &previous);
        self.active_profile = crate::activation::active_profile_name()?;
        if self.selected >= self.profiles.len() {
            self.selected = self.profiles.len().saturating_sub(1);
        }
        self.force_status_refresh();
        self.start_update_check();
        Ok(())
    }

    pub fn current_name(&self) -> Option<String> {
        self.profiles.get(self.selected).map(|p| p.name.clone())
    }

    pub fn move_down(&mut self) {
        if !self.profiles.is_empty() {
            self.selected = (self.selected + 1).min(self.profiles.len() - 1);
        }
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_history_down(&mut self) {
        if !self.history_sessions.is_empty() {
            self.selected_history =
                (self.selected_history + 1).min(self.history_sessions.len() - 1);
        }
    }

    pub fn move_history_up(&mut self) {
        self.selected_history = self.selected_history.saturating_sub(1);
    }

    pub fn current_history_session(&self) -> Option<process::HistorySession> {
        self.history_sessions.get(self.selected_history).cloned()
    }

    pub fn toggle_history_path_scope(&mut self) {
        self.history_all_paths = !self.history_all_paths;
        self.apply_history_filter();
    }

    pub fn history_scope_label(&self) -> &'static str {
        if self.history_all_paths {
            "all paths"
        } else {
            "current path"
        }
    }

    pub fn current_continue_profile_name(&self) -> Option<String> {
        self.profiles
            .get(self.selected_continue_profile)
            .map(|profile| profile.name.clone())
    }

    pub fn refresh_shared_accounts(&mut self) -> Result<()> {
        self.shared_accounts = crate::shared_account::list_shared_accounts()?;
        if self.selected_shared_account >= self.shared_accounts.len() {
            self.selected_shared_account = self.shared_accounts.len().saturating_sub(1);
        }
        Ok(())
    }

    pub fn current_shared_account_name(&self) -> Option<String> {
        self.shared_accounts
            .get(self.selected_shared_account)
            .map(|account| account.name.clone())
    }

    pub fn shared_account_for_profile(
        &self,
        name: &str,
    ) -> Option<&crate::shared_account::SharedAccountInfo> {
        self.shared_accounts
            .iter()
            .find(|account| account.name == name)
    }

    pub fn move_shared_account_down(&mut self) {
        if !self.shared_accounts.is_empty() {
            self.selected_shared_account =
                (self.selected_shared_account + 1).min(self.shared_accounts.len() - 1);
        }
    }

    pub fn move_shared_account_up(&mut self) {
        self.selected_shared_account = self.selected_shared_account.saturating_sub(1);
    }

    pub fn move_continue_profile_down(&mut self) {
        if !self.profiles.is_empty() {
            self.selected_continue_profile =
                (self.selected_continue_profile + 1).min(self.profiles.len() - 1);
        }
    }

    pub fn move_continue_profile_up(&mut self) {
        self.selected_continue_profile = self.selected_continue_profile.saturating_sub(1);
    }

    pub fn refresh_history_sessions(&mut self) -> Result<()> {
        self.start_history_refresh();
        Ok(())
    }

    pub fn poll_background(&mut self) {
        self.maybe_auto_refresh_status();
        if let Some(rx) = self.status_rx.take() {
            match rx.try_recv() {
                Ok(statuses) => {
                    for (name, status) in statuses {
                        if let Some(profile) = self
                            .profiles
                            .iter_mut()
                            .find(|profile| profile.name == name)
                        {
                            profile.apply_account_status(status);
                        }
                    }
                    self.status_loading = false;
                }
                Err(mpsc::TryRecvError::Empty) => self.status_rx = Some(rx),
                Err(mpsc::TryRecvError::Disconnected) => self.status_loading = false,
            }
        }
        if let Some(rx) = self.history_rx.take() {
            match rx.try_recv() {
                Ok(sessions) => {
                    self.all_history_sessions = sessions;
                    self.apply_history_filter();
                    self.history_loading = false;
                }
                Err(mpsc::TryRecvError::Empty) => self.history_rx = Some(rx),
                Err(mpsc::TryRecvError::Disconnected) => self.history_loading = false,
            }
        }
        if let Some(rx) = self.update_rx.take() {
            match rx.try_recv() {
                Ok(Ok(info)) => {
                    self.update_checking = false;
                    self.update_error = None;
                    self.update_info = info;
                }
                Ok(Err(err)) => {
                    self.update_checking = false;
                    self.update_info = None;
                    self.update_error = Some(err);
                }
                Err(mpsc::TryRecvError::Empty) => self.update_rx = Some(rx),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.update_checking = false;
                    self.update_error = Some("Update check stopped before it finished".into());
                }
            }
        }
        if self.update_info.is_some() && self.input_mode == InputMode::None {
            self.input_mode = InputMode::UpdatePrompt;
        }
    }

    pub fn start_status_refresh(&mut self) {
        if self.status_loading {
            return;
        }
        self.spawn_status_refresh();
    }

    pub fn force_status_refresh(&mut self) {
        self.status_rx = None;
        self.status_loading = false;
        self.spawn_status_refresh();
    }

    fn spawn_status_refresh(&mut self) {
        let names: Vec<_> = self
            .profiles
            .iter()
            .filter(|profile| profile.logged_in)
            .map(|profile| profile.name.clone())
            .collect();
        let (tx, rx) = mpsc::channel();
        self.status_loading = true;
        self.last_status_refresh = Some(Instant::now());
        self.status_rx = Some(rx);
        thread::spawn(move || {
            let statuses = account_statuses(names);
            let _ = tx.send(statuses);
        });
    }

    pub fn start_history_refresh(&mut self) {
        let names: Vec<_> = self
            .profiles
            .iter()
            .map(|profile| profile.name.clone())
            .collect();
        let (tx, rx) = mpsc::channel();
        self.history_loading = true;
        self.history_rx = Some(rx);
        thread::spawn(move || {
            let sessions = all_history_sessions(names);
            let _ = tx.send(sessions);
        });
    }

    fn apply_history_filter(&mut self) {
        self.history_sessions = filter_history_sessions(
            &self.all_history_sessions,
            self.history_all_paths,
            self.history_cwd.as_deref(),
        );
        self.selected_history = self
            .selected_history
            .min(self.history_sessions.len().saturating_sub(1));
    }

    pub fn start_update_check(&mut self) {
        if self.update_checking {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.update_checking = true;
        self.update_error = None;
        self.update_rx = Some(rx);
        thread::spawn(move || {
            let info = update::check_for_update().map_err(|err| err.to_string());
            let _ = tx.send(info);
        });
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = msg.into();
        self.input_mode = InputMode::Message;
    }

    pub fn activate_current_profile(&mut self) -> Result<()> {
        let Some(name) = self.current_name() else {
            self.set_message("No account selected");
            return Ok(());
        };
        let result = crate::activation::activate_profile(&name)?;
        self.active_profile = Some(name.clone());
        self.set_message(format!(
            "Activated account {name}\nCODEX_HOME={}\nRestart Codex Desktop if it is already running.",
            result.profile_path.display()
        ));
        Ok(())
    }

    pub fn cycle_theme(&mut self) -> Result<()> {
        self.theme_preference = self.theme_preference.cycle();
        self.theme = Theme::from_preference(self.theme_preference);
        save_theme_preference(self.theme_preference)?;
        Ok(())
    }

    pub fn increase_quota_refresh_interval(&mut self) -> Result<()> {
        self.quota_refresh_secs = (self.quota_refresh_secs + 15).min(3600);
        self.save_settings()
    }

    pub fn decrease_quota_refresh_interval(&mut self) -> Result<()> {
        self.quota_refresh_secs = self.quota_refresh_secs.saturating_sub(15).max(15);
        self.save_settings()
    }

    pub fn quota_refresh_label(&self) -> String {
        if self.quota_refresh_secs < 60 {
            format!("{}s", self.quota_refresh_secs)
        } else if self.quota_refresh_secs % 60 == 0 {
            format!("{}m", self.quota_refresh_secs / 60)
        } else {
            format!(
                "{}m {}s",
                self.quota_refresh_secs / 60,
                self.quota_refresh_secs % 60
            )
        }
    }

    fn maybe_auto_refresh_status(&mut self) {
        if self.status_loading || self.quota_refresh_secs == 0 {
            return;
        }
        let Some(last) = self.last_status_refresh else {
            self.start_status_refresh();
            return;
        };
        if last.elapsed() >= Duration::from_secs(self.quota_refresh_secs) {
            self.start_status_refresh();
        }
    }

    fn save_settings(&self) -> Result<()> {
        let mut config = crate::config::load()?;
        config.quota_refresh_secs = self.quota_refresh_secs;
        config.proxy = self.proxy.clone();
        crate::config::save(&config)
    }

    pub fn cycle_proxy_mode(&mut self) -> Result<()> {
        let mode = match self.proxy.mode {
            crate::config::ProxyMode::Inherit => crate::config::ProxyMode::Off,
            crate::config::ProxyMode::Off => crate::config::ProxyMode::Custom,
            crate::config::ProxyMode::Custom => crate::config::ProxyMode::Inherit,
        };
        let mut candidate = self.proxy.clone();
        candidate.mode = mode;
        crate::proxy::validate(&candidate)?;
        self.proxy = candidate;
        self.save_settings()
    }

    pub fn save_proxy_value(&mut self, mode: InputMode, value: String) -> Result<()> {
        let mut candidate = self.proxy.clone();
        match mode {
            InputMode::ProxyHttp => candidate.http = value,
            InputMode::ProxyHttps => candidate.https = value,
            InputMode::ProxyAll => candidate.all = value,
            InputMode::ProxyNoProxy => candidate.no_proxy = value,
            _ => return Ok(()),
        }
        if !candidate.http.is_empty() || !candidate.https.is_empty() || !candidate.all.is_empty() {
            let original_mode = candidate.mode;
            candidate.mode = crate::config::ProxyMode::Custom;
            crate::proxy::validate(&candidate)?;
            candidate.mode = original_mode;
        }
        self.proxy = candidate;
        self.save_settings()
    }

    pub fn proxy_value(&self, mode: InputMode) -> &str {
        match mode {
            InputMode::ProxyHttp => &self.proxy.http,
            InputMode::ProxyHttps => &self.proxy.https,
            InputMode::ProxyAll => &self.proxy.all,
            InputMode::ProxyNoProxy => &self.proxy.no_proxy,
            _ => "",
        }
    }

    pub fn theme_label(&self) -> String {
        match self.theme_preference {
            ThemePreference::Auto => {
                format!("auto/{}", Theme::detected_preference().as_str())
            }
            preference => preference.as_str().to_string(),
        }
    }
}

fn load_theme_preference() -> ThemePreference {
    let Ok(paths) = crate::config::paths() else {
        return ThemePreference::Auto;
    };
    let Ok(value) = std::fs::read_to_string(paths.root.join("theme")) else {
        return ThemePreference::Auto;
    };
    ThemePreference::parse(&value).unwrap_or(ThemePreference::Auto)
}

fn save_theme_preference(preference: ThemePreference) -> Result<()> {
    let paths = crate::config::init()?;
    std::fs::write(
        paths.root.join("theme"),
        format!("{}\n", preference.as_str()),
    )?;
    Ok(())
}

fn account_statuses(names: Vec<String>) -> Vec<(String, process::AccountStatus)> {
    let handles: Vec<_> = names
        .into_iter()
        .map(|name| {
            thread::spawn(move || {
                let status = process::codex_account_status(&name).ok()?;
                Some((name, status))
            })
        })
        .collect();
    handles
        .into_iter()
        .filter_map(|handle| handle.join().ok().flatten())
        .collect()
}

fn preserve_account_status(
    profiles: &mut [profile::ProfileInfo],
    previous: &[profile::ProfileInfo],
) {
    for profile in profiles {
        let Some(old) = previous.iter().find(|old| old.name == profile.name) else {
            continue;
        };
        profile.plan_type = profile.plan_type.clone().or_else(|| old.plan_type.clone());
        profile.limit_5h_label = old.limit_5h_label.clone();
        profile.limit_5h_remaining = old.limit_5h_remaining;
        profile.limit_7day_label = old.limit_7day_label.clone();
        profile.limit_7day_remaining = old.limit_7day_remaining;
        profile.limit_5h_resets_at = old.limit_5h_resets_at;
        profile.limit_7day_resets_at = old.limit_7day_resets_at;
    }
}

fn all_history_sessions(names: Vec<String>) -> Vec<process::HistorySession> {
    const HISTORY_LIMIT: usize = 1000;
    let mut handles: Vec<_> = names
        .into_iter()
        .map(|name| {
            thread::spawn(move || {
                process::codex_history_sessions(&name, HISTORY_LIMIT).unwrap_or_default()
            })
        })
        .collect();
    if let Some(home) = default_codex_history_home() {
        handles.push(thread::spawn(move || {
            process::codex_history_sessions_from_home("~/.codex", home, false, HISTORY_LIMIT)
                .unwrap_or_default()
        }));
    }
    let mut sessions: Vec<_> = handles
        .into_iter()
        .filter_map(|handle| handle.join().ok())
        .flatten()
        .collect();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions
}

fn default_codex_history_home() -> Option<PathBuf> {
    let home = profile::default_codex_home().ok()?;
    home.join("auth.json").is_file().then_some(home)
}

fn filter_history_sessions(
    sessions: &[process::HistorySession],
    all_paths: bool,
    current_dir: Option<&Path>,
) -> Vec<process::HistorySession> {
    if all_paths {
        return sessions.to_vec();
    }
    let Some(current_dir) = current_dir else {
        return Vec::new();
    };
    sessions
        .iter()
        .filter(|session| {
            session
                .cwd
                .as_deref()
                .map(|cwd| cwd_matches_current_path(cwd, current_dir))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

fn cwd_matches_current_path(cwd: &str, current_dir: &Path) -> bool {
    let cwd = Path::new(cwd);
    if cwd == current_dir {
        return true;
    }
    match (cwd.canonicalize(), current_dir.canonicalize()) {
        (Ok(cwd), Ok(current_dir)) => cwd == current_dir,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_history_to_current_path_by_default() {
        let sessions = vec![
            history_session("current", Some("/repo"), 200),
            history_session("other", Some("/other"), 300),
            history_session("unknown", None, 400),
        ];

        let filtered =
            filter_history_sessions(&sessions, false, Some(std::path::Path::new("/repo")));

        let ids: Vec<_> = filtered
            .into_iter()
            .map(|session| session.session_id)
            .collect();
        assert_eq!(ids, vec!["current"]);
    }

    #[test]
    fn shows_all_history_paths_when_scope_is_all() {
        let sessions = vec![
            history_session("current", Some("/repo"), 200),
            history_session("other", Some("/other"), 300),
        ];

        let filtered =
            filter_history_sessions(&sessions, true, Some(std::path::Path::new("/repo")));

        let ids: Vec<_> = filtered
            .into_iter()
            .map(|session| session.session_id)
            .collect();
        assert_eq!(ids, vec!["current", "other"]);
    }

    fn history_session(id: &str, cwd: Option<&str>, updated_at: i64) -> process::HistorySession {
        process::HistorySession {
            profile: "work".into(),
            codex_home: PathBuf::from("/tmp/work"),
            is_codexhub_profile: true,
            session_id: id.into(),
            title: id.into(),
            cwd: cwd.map(str::to_string),
            path: None,
            updated_at,
        }
    }
}
