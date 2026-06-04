use super::{
    app::App,
    screens::{InputMode, Screen},
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::process::Command;
use std::time::Duration;

pub fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        app.poll_background();
        terminal.draw(|frame| super::ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if handle_key(terminal, app, key)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn handle_key(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<bool> {
    if app.input_mode != InputMode::None {
        return handle_input(terminal, app, key);
    }
    match app.screen {
        Screen::List => handle_list(terminal, app, key),
        Screen::Doctor => handle_doctor(app, key),
        Screen::History => handle_history(terminal, app, key),
        Screen::Remote => handle_remote(app, key),
        Screen::SharedAccounts => handle_shared_accounts(app, key),
        Screen::Settings => handle_settings(app, key),
    }
}

fn handle_list(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('t') => app.cycle_theme()?,
        KeyCode::Down => app.move_down(),
        KeyCode::Up => app.move_up(),
        KeyCode::Enter => app.activate_current_profile()?,
        KeyCode::Char('n') => start_input(app, InputMode::AddAccountMethod),
        KeyCode::Char('d') => start_input(app, InputMode::DeleteConfirm),
        KeyCode::Char('l') => start_input(app, InputMode::LoginMethodForSelected),
        KeyCode::Char('r') => app.refresh_profiles_now()?,
        KeyCode::Char('R') => app.screen = Screen::Remote,
        KeyCode::Char('S') => {
            app.refresh_shared_accounts()?;
            app.screen = Screen::SharedAccounts;
        }
        KeyCode::Char('s') => app.screen = Screen::Settings,
        KeyCode::Char('o') => external_open(terminal, app)?,
        KeyCode::Char('h') => {
            app.refresh_history_sessions()?;
            app.screen = Screen::History;
        }
        KeyCode::Char('D') => {
            app.doctor_checks = crate::doctor::run(false)?;
            app.screen = Screen::Doctor;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_remote(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => app.screen = Screen::List,
        KeyCode::Char('t') => app.cycle_theme()?,
        _ => {}
    }
    Ok(false)
}

fn handle_shared_accounts(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => app.screen = Screen::List,
        KeyCode::Char('t') => app.cycle_theme()?,
        KeyCode::Down => app.move_down(),
        KeyCode::Up => app.move_up(),
        KeyCode::Char('S') => start_input(app, InputMode::ShareTargetUser),
        KeyCode::Char('u') => start_input(app, InputMode::RemoveSharedAccountConfirm),
        KeyCode::Char('i') => {
            app.refresh_shared_accounts()?;
            app.input_mode = InputMode::ImportSharedAccount;
        }
        KeyCode::Char('r') => {
            app.refresh_profiles_now()?;
            app.refresh_shared_accounts()?;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_settings(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => app.screen = Screen::List,
        KeyCode::Char('t') => app.cycle_theme()?,
        KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Right | KeyCode::Up => {
            app.increase_quota_refresh_interval()?;
        }
        KeyCode::Char('-') | KeyCode::Left | KeyCode::Down => {
            app.decrease_quota_refresh_interval()?;
        }
        KeyCode::Char('r') => app.force_status_refresh(),
        _ => {}
    }
    Ok(false)
}

fn handle_doctor(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => app.screen = Screen::List,
        KeyCode::Char('t') => app.cycle_theme()?,
        KeyCode::Char('r') => app.doctor_checks = crate::doctor::run(false)?,
        _ => {}
    }
    Ok(false)
}

fn handle_history(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => app.screen = Screen::List,
        KeyCode::Char('t') => app.cycle_theme()?,
        KeyCode::Down => app.move_history_down(),
        KeyCode::Up => app.move_history_up(),
        KeyCode::Char('a') => app.toggle_history_path_scope(),
        KeyCode::Char('r') => app.refresh_history_sessions()?,
        KeyCode::Char('d') => start_input(app, InputMode::DeleteSessionConfirm),
        KeyCode::Char('c') => start_continue_profile_select(app),
        KeyCode::Enter => external_resume(terminal, app)?,
        _ => {}
    }
    Ok(false)
}

fn handle_input(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Esc => {
            app.input.clear();
            if app.input_mode == InputMode::UpdatePrompt {
                app.update_info = None;
            }
            app.pending_login_method = None;
            app.pending_share_plan = None;
            app.input_mode = InputMode::None;
        }
        KeyCode::Char('1') if app.input_mode == InputMode::AddAccountMethod => {
            app.input_mode = InputMode::LoginMethodForNewAccount;
        }
        KeyCode::Char('2') if app.input_mode == InputMode::AddAccountMethod => {
            start_input(app, InputMode::ImportDefault);
        }
        KeyCode::Char('3') if app.input_mode == InputMode::AddAccountMethod => {
            start_input(app, InputMode::ImportSub2);
        }
        KeyCode::Char('4') if app.input_mode == InputMode::AddAccountMethod => {
            app.refresh_shared_accounts()?;
            app.input_mode = InputMode::ImportSharedAccount;
        }
        KeyCode::Char('1') if app.input_mode == InputMode::LoginMethodForNewAccount => {
            app.pending_login_method = Some(crate::process::LoginMethod::DeviceCode);
            start_input(app, InputMode::NewLoginProfileName);
        }
        KeyCode::Char('2') if app.input_mode == InputMode::LoginMethodForNewAccount => {
            app.pending_login_method = Some(crate::process::LoginMethod::Web);
            start_input(app, InputMode::NewLoginProfileName);
        }
        KeyCode::Char('1') if app.input_mode == InputMode::LoginMethodForSelected => {
            external_relogin(terminal, app, crate::process::LoginMethod::DeviceCode)?;
        }
        KeyCode::Char('2') if app.input_mode == InputMode::LoginMethodForSelected => {
            external_relogin(terminal, app, crate::process::LoginMethod::Web)?;
        }
        KeyCode::Enter => submit_input(terminal, app)?,
        KeyCode::Down if app.input_mode == InputMode::ImportSharedAccount => {
            app.move_shared_account_down();
        }
        KeyCode::Up if app.input_mode == InputMode::ImportSharedAccount => {
            app.move_shared_account_up();
        }
        KeyCode::Down if app.input_mode == InputMode::ContinueProfile => {
            app.move_continue_profile_down();
        }
        KeyCode::Up if app.input_mode == InputMode::ContinueProfile => {
            app.move_continue_profile_up();
        }
        KeyCode::Char('y') if app.input_mode == InputMode::UpdatePrompt => {
            install_update(terminal, app)?;
        }
        KeyCode::Char('n') if app.input_mode == InputMode::UpdatePrompt => {
            app.update_info = None;
            app.input_mode = InputMode::None;
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
        KeyCode::Char(ch) => {
            if matches!(
                app.input_mode,
                InputMode::NewLoginProfileName
                    | InputMode::ImportDefault
                    | InputMode::ImportSub2
                    | InputMode::ShareTargetUser
                    | InputMode::RemoveSharedAccountConfirm
                    | InputMode::DeleteConfirm
                    | InputMode::DeleteSessionConfirm
            ) {
                app.input.push(ch);
            }
        }
        _ => {}
    }
    Ok(false)
}

fn submit_input(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    match app.input_mode {
        InputMode::NewLoginProfileName => {
            let name = app.input.trim().to_string();
            let method = app
                .pending_login_method
                .take()
                .unwrap_or(crate::process::LoginMethod::DeviceCode);
            crate::profile::create(&name, false)?;
            crate::activation::activate_profile(&name)?;
            app.active_profile = Some(name.clone());
            let status = run_suspended(terminal, || crate::process::codex_login(&name, method))?;
            app.refresh_profiles()?;
            if status == 0 {
                app.set_message(format!("Added account {name} with {}", method.label()));
            } else {
                app.set_message(format!(
                    "Created account {name}, but {} exited with status {status}",
                    method.label()
                ));
            }
        }
        InputMode::ImportDefault => {
            let explicit = app.input.trim();
            let (name, _) =
                crate::profile::import_default((!explicit.is_empty()).then_some(explicit))?;
            app.refresh_profiles()?;
            app.set_message(format!("Imported ~/.codex as account {name}"));
        }
        InputMode::ImportSub2 => {
            let json = app.input.trim();
            let (name, _) = crate::profile::import_sub2_json(json, None)?;
            app.refresh_profiles()?;
            app.set_message(format!("Imported JSON as account {name}"));
        }
        InputMode::ShareTargetUser => {
            let Some(name) = app.current_name() else {
                app.set_message("No account selected");
                return Ok(());
            };
            let target_user = app.input.trim().to_string();
            let plan = crate::shared_account::plan_share_account(&name, &target_user)?;
            if plan.needs_sudo {
                app.pending_share_plan = Some(plan);
                app.input_mode = InputMode::ShareNeedsSudo;
                app.input.clear();
                return Ok(());
            }
            crate::shared_account::share_account(&name, &target_user)?;
            app.refresh_shared_accounts()?;
            app.set_message(format!("Shared account {name} with {target_user}"));
        }
        InputMode::ShareNeedsSudo => {
            external_share_sudo_helper(terminal, app)?;
        }
        InputMode::RemoveSharedAccountConfirm => {
            let Some(name) = app.current_name() else {
                app.set_message("No account selected");
                return Ok(());
            };
            if app.input.trim() == name {
                match crate::shared_account::remove_shared_account(&name) {
                    Ok(()) => {
                        app.refresh_shared_accounts()?;
                        app.set_message(format!("Removed shared account {name}"));
                    }
                    Err(err) => app.set_message(format!("Remove shared account failed: {err}")),
                }
            } else {
                app.set_message("Shared account removal cancelled");
            }
        }
        InputMode::ImportSharedAccount => {
            let Some(name) = app.current_shared_account_name() else {
                app.set_message("No shared account available");
                return Ok(());
            };
            crate::shared_account::import_shared_account(&name)?;
            app.refresh_profiles()?;
            app.set_message(format!("Imported shared account {name}"));
        }
        InputMode::DeleteConfirm => {
            let Some(name) = app.current_name() else {
                return Ok(());
            };
            if app.input.trim() == name {
                crate::profile::delete(&name)?;
                app.refresh_profiles()?;
                app.set_message(format!(
                    "Deleted account login for {name}; history preserved"
                ));
            } else {
                app.set_message("Deletion cancelled");
            }
        }
        InputMode::DeleteSessionConfirm => {
            let Some(session) = app.current_history_session() else {
                app.set_message("No history session selected");
                return Ok(());
            };
            if app.input.trim() == session.session_id {
                crate::process::delete_history_session(&session)?;
                app.refresh_history_sessions()?;
                app.set_message(format!("Deleted session {}", session.session_id));
            } else {
                app.set_message("Session deletion cancelled");
            }
        }
        InputMode::ContinueProfile => {
            external_continue_with_selected_profile(terminal, app)?;
            app.input_mode = InputMode::None;
            app.input.clear();
        }
        InputMode::UpdatePrompt => {
            install_update(terminal, app)?;
        }
        InputMode::Message => {
            app.input_mode = InputMode::None;
            app.input.clear();
        }
        InputMode::AddAccountMethod
        | InputMode::LoginMethodForNewAccount
        | InputMode::LoginMethodForSelected => {}
        InputMode::None => {}
    }
    if app.input_mode != InputMode::Message {
        app.input.clear();
        app.input_mode = InputMode::None;
    }
    Ok(())
}

fn start_input(app: &mut App, mode: InputMode) {
    app.input.clear();
    if app.current_name().is_none()
        && !matches!(
            mode,
            InputMode::AddAccountMethod
                | InputMode::LoginMethodForNewAccount
                | InputMode::NewLoginProfileName
                | InputMode::ImportDefault
                | InputMode::ImportSub2
                | InputMode::ShareTargetUser
                | InputMode::RemoveSharedAccountConfirm
                | InputMode::ImportSharedAccount
                | InputMode::DeleteSessionConfirm
                | InputMode::ContinueProfile
        )
    {
        app.set_message("No account selected");
    } else {
        app.input_mode = mode;
    }
}

fn external_share_sudo_helper(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let Some(plan) = app.pending_share_plan.clone() else {
        app.set_message("No pending shared account setup");
        return Ok(());
    };
    let command = crate::shared_account::tmux_helper_command(&plan);
    let status = run_suspended(terminal, || {
        let status = Command::new("sh").arg("-lc").arg(&command).status()?;
        Ok(status.code().unwrap_or(1))
    })?;
    app.pending_share_plan = None;
    app.refresh_shared_accounts().ok();
    if status == 0 {
        app.set_message(format!(
            "Shared account {} with {}",
            plan.profile, plan.target_user
        ));
    } else {
        app.set_message(format!("Shared account setup exited with status {status}"));
    }
    Ok(())
}

fn start_continue_profile_select(app: &mut App) {
    if app.profiles.is_empty() {
        app.set_message("No account available");
        return;
    }
    let selected = app
        .current_history_session()
        .and_then(|session| {
            app.profiles
                .iter()
                .position(|profile| profile.name != session.profile)
        })
        .unwrap_or(0);
    app.selected_continue_profile = selected;
    app.input_mode = InputMode::ContinueProfile;
}

fn external_open(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let Some(name) = app.current_name() else {
        app.set_message("No account selected");
        return Ok(());
    };
    crate::activation::activate_profile(&name)?;
    app.active_profile = Some(name.clone());
    let _status = run_suspended(terminal, || crate::process::codex_run(&name, &[]))?;
    app.refresh_profiles()?;
    Ok(())
}

fn external_relogin(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    method: crate::process::LoginMethod,
) -> Result<()> {
    let Some(name) = app.current_name() else {
        app.set_message("No account selected");
        return Ok(());
    };
    crate::activation::activate_profile(&name)?;
    app.active_profile = Some(name.clone());
    let status = run_suspended(terminal, || crate::process::codex_login(&name, method))?;
    app.refresh_profiles()?;
    if status == 0 {
        app.set_message(format!("Relogged account {name} with {}", method.label()));
    } else {
        app.set_message(format!(
            "Relogin for {name} with {} exited with status {status}",
            method.label()
        ));
    }
    Ok(())
}

fn external_resume(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let Some(session) = app.current_history_session() else {
        app.set_message("No history session selected");
        return Ok(());
    };
    if session.is_codexhub_profile {
        crate::activation::activate_profile(&session.profile)?;
        app.active_profile = Some(session.profile.clone());
    }
    let _status = run_suspended(terminal, || crate::process::codex_resume_session(&session))?;
    app.refresh_profiles()?;
    app.refresh_history_sessions()?;
    Ok(())
}

fn external_continue_with_selected_profile(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let Some(session) = app.current_history_session() else {
        app.set_message("No history session selected");
        return Ok(());
    };
    let Some(target) = app.current_continue_profile_name() else {
        app.set_message("No target profile selected");
        return Ok(());
    };
    crate::profile::ensure_exists(&target)?;
    crate::activation::activate_profile(&target)?;
    app.active_profile = Some(target.clone());
    let _status = run_suspended(terminal, || {
        crate::process::codex_resume_copied_session(&session, &target)
    })?;
    app.refresh_profiles()?;
    app.refresh_history_sessions()?;
    Ok(())
}

fn install_update(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let Some(info) = app.update_info.clone() else {
        app.input_mode = InputMode::None;
        return Ok(());
    };
    let result = run_suspended(terminal, || {
        crate::update::install_update(&info.repo_path)?;
        Ok(0)
    });
    app.update_info = None;
    app.input_mode = InputMode::None;
    match result {
        Ok(_) => app.set_message("Update installed. Restart CodexHub to use the new binary."),
        Err(err) => app.set_message(format!("Update failed: {err}")),
    }
    Ok(())
}

fn run_suspended(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    run: impl FnOnce() -> Result<i32>,
) -> Result<i32> {
    super::suspend_terminal(terminal)?;
    let result = run();
    let resume_result = super::resume_terminal(terminal);
    match (result, resume_result) {
        (Ok(status), Ok(())) => Ok(status),
        (Err(err), Ok(())) => Err(err),
        (Ok(_), Err(err)) | (Err(_), Err(err)) => Err(err),
    }
}
