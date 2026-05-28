use super::screens::{InputMode, Screen};
use crate::{doctor, process, profile};
use anyhow::Result;
use std::thread;

pub struct App {
    pub screen: Screen,
    pub input_mode: InputMode,
    pub profiles: Vec<profile::ProfileInfo>,
    pub selected: usize,
    pub input: String,
    pub message: String,
    pub doctor_checks: Vec<doctor::Check>,
    pub history_sessions: Vec<process::HistorySession>,
    pub selected_history: usize,
}

impl App {
    pub fn new() -> Result<Self> {
        crate::config::init()?;
        let profiles = profiles_with_account_status()?;
        Ok(Self {
            screen: Screen::List,
            input_mode: InputMode::None,
            profiles,
            selected: 0,
            input: String::new(),
            message: String::new(),
            doctor_checks: Vec::new(),
            history_sessions: Vec::new(),
            selected_history: 0,
        })
    }

    pub fn refresh_profiles(&mut self) -> Result<()> {
        self.profiles = profiles_with_account_status()?;
        if self.selected >= self.profiles.len() {
            self.selected = self.profiles.len().saturating_sub(1);
        }
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

    pub fn refresh_history_sessions(&mut self) -> Result<()> {
        self.history_sessions = all_history_sessions(&self.profiles);
        self.selected_history = self
            .selected_history
            .min(self.history_sessions.len().saturating_sub(1));
        Ok(())
    }

    pub fn set_message(&mut self, msg: impl Into<String>) {
        self.message = msg.into();
        self.input_mode = InputMode::Message;
    }
}

fn profiles_with_account_status() -> Result<Vec<profile::ProfileInfo>> {
    let mut profiles = profile::list()?;
    let handles: Vec<_> = profiles
        .iter()
        .filter(|profile| profile.logged_in)
        .map(|profile| {
            let name = profile.name.clone();
            thread::spawn(move || {
                let status = process::codex_account_status(&name).ok()?;
                Some((name, status))
            })
        })
        .collect();

    for handle in handles {
        let Some((name, status)) = handle.join().ok().flatten() else {
            continue;
        };
        if let Some(profile) = profiles.iter_mut().find(|profile| profile.name == name) {
            profile.apply_account_status(status);
        }
    }

    Ok(profiles)
}

fn all_history_sessions(profiles: &[profile::ProfileInfo]) -> Vec<process::HistorySession> {
    let handles: Vec<_> = profiles
        .iter()
        .filter(|profile| profile.logged_in)
        .map(|profile| {
            let name = profile.name.clone();
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
