use super::{
    app::App,
    screens::{InputMode, Screen},
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
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
        Screen::Detail => handle_detail(terminal, app, key),
        Screen::Doctor => handle_doctor(app, key),
        Screen::History => handle_history(terminal, app, key),
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
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Enter => app.screen = Screen::Detail,
        KeyCode::Char('n') => start_input(app, InputMode::NewProfile),
        KeyCode::Char('i') => start_input(app, InputMode::ImportDefault),
        KeyCode::Char('2') => start_input(app, InputMode::ImportSub2),
        KeyCode::Char('d') => start_input(app, InputMode::DeleteConfirm),
        KeyCode::Char('a') => app.activate_current_profile()?,
        KeyCode::Char('l') => external(terminal, app, External::Login)?,
        KeyCode::Char('r') => external(terminal, app, External::Run)?,
        KeyCode::Char('e') => start_input(app, InputMode::ExecPrompt),
        KeyCode::Char('h') => {
            app.refresh_history_sessions()?;
            app.screen = Screen::History;
        }
        KeyCode::Char('s') => app.input_mode = InputMode::ShareConfirm,
        KeyCode::Char('u') => app.input_mode = InputMode::UnshareConfirm,
        KeyCode::Char('D') => {
            app.doctor_checks = crate::doctor::run(false)?;
            app.screen = Screen::Doctor;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_detail(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('b') => app.screen = Screen::List,
        KeyCode::Char('t') => app.cycle_theme()?,
        KeyCode::Char('a') => app.activate_current_profile()?,
        KeyCode::Char('l') => external(terminal, app, External::Login)?,
        KeyCode::Char('r') => external(terminal, app, External::Run)?,
        KeyCode::Char('e') => start_input(app, InputMode::ExecPrompt),
        KeyCode::Char('s') => app.input_mode = InputMode::ShareConfirm,
        KeyCode::Char('u') => app.input_mode = InputMode::UnshareConfirm,
        KeyCode::Char('D') => {
            app.doctor_checks = crate::doctor::run(false)?;
            app.screen = Screen::Doctor;
        }
        _ => {}
    }
    Ok(false)
}

fn handle_doctor(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('b') => app.screen = Screen::List,
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
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('b') => app.screen = Screen::List,
        KeyCode::Char('t') => app.cycle_theme()?,
        KeyCode::Down | KeyCode::Char('j') => app.move_history_down(),
        KeyCode::Up | KeyCode::Char('k') => app.move_history_up(),
        KeyCode::Char('a') => app.toggle_history_path_scope(),
        KeyCode::Char('r') => app.refresh_history_sessions()?,
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
            app.input_mode = InputMode::None;
        }
        KeyCode::Enter => submit_input(terminal, app)?,
        KeyCode::Down | KeyCode::Char('j') if app.input_mode == InputMode::ContinueProfile => {
            app.move_continue_profile_down();
        }
        KeyCode::Up | KeyCode::Char('k') if app.input_mode == InputMode::ContinueProfile => {
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
                InputMode::NewProfile
                    | InputMode::ImportDefault
                    | InputMode::ImportSub2
                    | InputMode::DeleteConfirm
                    | InputMode::ExecPrompt
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
        InputMode::NewProfile => {
            let name = app.input.trim().to_string();
            crate::profile::create(&name, false)?;
            app.refresh_profiles()?;
            app.set_message(format!("Created profile {name}"));
        }
        InputMode::ImportDefault => {
            let explicit = app.input.trim();
            let (name, _) =
                crate::profile::import_default((!explicit.is_empty()).then_some(explicit))?;
            app.refresh_profiles()?;
            app.set_message(format!("Imported ~/.codex as profile {name}"));
        }
        InputMode::ImportSub2 => {
            let json = app.input.trim();
            let (name, _) = crate::profile::import_sub2_json(json, None)?;
            app.refresh_profiles()?;
            app.set_message(format!("Imported sub2 JSON as profile {name}"));
        }
        InputMode::DeleteConfirm => {
            let Some(name) = app.current_name() else {
                return Ok(());
            };
            if app.input.trim() == name {
                crate::profile::delete(&name)?;
                app.refresh_profiles()?;
                app.set_message(format!("Deleted profile {name}"));
            } else {
                app.set_message("Deletion cancelled");
            }
        }
        InputMode::ExecPrompt => {
            external_with_prompt(terminal, app)?;
            app.input_mode = InputMode::None;
            app.input.clear();
        }
        InputMode::ContinueProfile => {
            external_continue_with_selected_profile(terminal, app)?;
            app.input_mode = InputMode::None;
            app.input.clear();
        }
        InputMode::ShareConfirm => {
            let Some(name) = app.current_name() else {
                return Ok(());
            };
            crate::shared::share_cache(&name)?;
            app.refresh_profiles()?;
            app.set_message(format!("Shared cache enabled for {name}"));
        }
        InputMode::UnshareConfirm => {
            let Some(name) = app.current_name() else {
                return Ok(());
            };
            crate::shared::unshare_cache(&name, false, true)?;
            app.refresh_profiles()?;
            app.set_message(format!("Shared cache disabled for {name}"));
        }
        InputMode::UpdatePrompt => {
            install_update(terminal, app)?;
        }
        InputMode::Message => {
            app.input_mode = InputMode::None;
            app.input.clear();
        }
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
            InputMode::NewProfile
                | InputMode::ImportDefault
                | InputMode::ImportSub2
                | InputMode::ContinueProfile
        )
    {
        app.set_message("No profile selected");
    } else {
        app.input_mode = mode;
    }
}

fn start_continue_profile_select(app: &mut App) {
    if app.profiles.is_empty() {
        app.set_message("No profile available");
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

enum External {
    Login,
    Run,
}

fn external(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    action: External,
) -> Result<()> {
    let Some(name) = app.current_name() else {
        app.set_message("No profile selected");
        return Ok(());
    };
    crate::activation::activate_profile(&name)?;
    app.active_profile = Some(name.clone());
    let _status = run_suspended(terminal, || match action {
        External::Login => crate::process::codex_login(&name),
        External::Run => crate::process::codex_run(&name, &[]),
    })?;
    app.refresh_profiles()?;
    Ok(())
}

fn external_with_prompt(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let Some(name) = app.current_name() else {
        app.set_message("No profile selected");
        return Ok(());
    };
    let prompt = app.input.clone();
    crate::activation::activate_profile(&name)?;
    app.active_profile = Some(name.clone());
    let _status = run_suspended(terminal, || crate::process::codex_exec(&name, &[prompt]))?;
    app.refresh_profiles()?;
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
    crate::activation::activate_profile(&session.profile)?;
    app.active_profile = Some(session.profile.clone());
    let _status = run_suspended(terminal, || {
        crate::process::codex_resume(&session.profile, &session.session_id)
    })?;
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
