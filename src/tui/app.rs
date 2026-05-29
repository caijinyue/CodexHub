use super::screens::{InputMode, Screen};
use crate::{doctor, process, profile, update};
use anyhow::Result;
use std::sync::mpsc::{self, Receiver};
use std::thread;

pub struct App {
    pub screen: Screen,
    pub input_mode: InputMode,
    pub profiles: Vec<profile::ProfileInfo>,
    pub active_profile: Option<String>,
    pub selected: usize,
    pub input: String,
    pub message: String,
    pub doctor_checks: Vec<doctor::Check>,
    pub history_sessions: Vec<process::HistorySession>,
    pub selected_history: usize,
    pub selected_continue_profile: usize,
    pub update_info: Option<update::UpdateInfo>,
    pub status_loading: bool,
    pub history_loading: bool,
    pub update_checking: bool,
    status_rx: Option<Receiver<Vec<(String, process::AccountStatus)>>>,
    history_rx: Option<Receiver<Vec<process::HistorySession>>>,
    update_rx: Option<Receiver<Option<update::UpdateInfo>>>,
}

impl App {
    pub fn new() -> Result<Self> {
        crate::config::init()?;
        let profiles = profile::list()?;
        let active_profile = crate::activation::active_profile_name()?;
        let mut app = Self {
            screen: Screen::List,
            input_mode: InputMode::None,
            profiles,
            active_profile,
            selected: 0,
            input: String::new(),
            message: String::new(),
            doctor_checks: Vec::new(),
            history_sessions: Vec::new(),
            selected_history: 0,
            selected_continue_profile: 0,
            update_info: None,
            status_loading: false,
            history_loading: false,
            update_checking: false,
            status_rx: None,
            history_rx: None,
            update_rx: None,
        };
        app.start_status_refresh();
        app.start_update_check();
        Ok(app)
    }

    pub fn refresh_profiles(&mut self) -> Result<()> {
        self.profiles = profile::list()?;
        self.active_profile = crate::activation::active_profile_name()?;
        if self.selected >= self.profiles.len() {
            self.selected = self.profiles.len().saturating_sub(1);
        }
        self.start_status_refresh();
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

    pub fn current_continue_profile_name(&self) -> Option<String> {
        self.profiles
            .get(self.selected_continue_profile)
            .map(|profile| profile.name.clone())
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
                    self.history_sessions = sessions;
                    self.selected_history = self
                        .selected_history
                        .min(self.history_sessions.len().saturating_sub(1));
                    self.history_loading = false;
                }
                Err(mpsc::TryRecvError::Empty) => self.history_rx = Some(rx),
                Err(mpsc::TryRecvError::Disconnected) => self.history_loading = false,
            }
        }
        if let Some(rx) = self.update_rx.take() {
            match rx.try_recv() {
                Ok(info) => {
                    self.update_checking = false;
                    self.update_info = info;
                }
                Err(mpsc::TryRecvError::Empty) => self.update_rx = Some(rx),
                Err(mpsc::TryRecvError::Disconnected) => self.update_checking = false,
            }
        }
        if self.update_info.is_some() && self.input_mode == InputMode::None {
            self.input_mode = InputMode::UpdatePrompt;
        }
    }

    pub fn start_status_refresh(&mut self) {
        let names: Vec<_> = self
            .profiles
            .iter()
            .filter(|profile| profile.logged_in)
            .map(|profile| profile.name.clone())
            .collect();
        let (tx, rx) = mpsc::channel();
        self.status_loading = true;
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
            .filter(|profile| profile.logged_in)
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

    pub fn start_update_check(&mut self) {
        if self.update_checking {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.update_checking = true;
        self.update_rx = Some(rx);
        thread::spawn(move || {
            let info = update::check_for_update().ok().flatten();
            let _ = tx.send(info);
        });
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = msg.into();
        self.input_mode = InputMode::Message;
    }

    pub fn activate_current_profile(&mut self) -> Result<()> {
        let Some(name) = self.current_name() else {
            self.set_message("No profile selected");
            return Ok(());
        };
        let result = crate::activation::activate_profile(&name)?;
        self.active_profile = Some(name.clone());
        self.set_message(format!(
            "Activated {name}\nCODEX_HOME={}\nRestart Codex Desktop if it is already running.",
            result.profile_path.display()
        ));
        Ok(())
    }
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

fn all_history_sessions(names: Vec<String>) -> Vec<process::HistorySession> {
    let handles: Vec<_> = names
        .into_iter()
        .map(|name| {
            thread::spawn(move || process::codex_history_sessions(&name, 200).unwrap_or_default())
        })
        .collect();
    let mut sessions: Vec<_> = handles
        .into_iter()
        .filter_map(|handle| handle.join().ok())
        .flatten()
        .collect();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions
}
