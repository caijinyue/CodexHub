use super::screens::{InputMode, Screen};
use crate::{doctor, profile};
use anyhow::Result;

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
        Ok(Self {
            screen: Screen::List,
            input_mode: InputMode::None,
            profiles: profile::list()?,
            selected: 0,
            input: String::new(),
            message: String::new(),
            doctor_checks: Vec::new(),
        })
    }

    pub fn refresh_profiles(&mut self) -> Result<()> {
        self.profiles = profile::list()?;
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
