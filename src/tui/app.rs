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
