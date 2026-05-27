use super::{
    app::App,
    screens::{InputMode, Screen},
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Write};
use std::time::Duration;

pub fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
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
    }
}

fn handle_list(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Down | KeyCode::Char('j') => app.move_down(),
        KeyCode::Up | KeyCode::Char('k') => app.move_up(),
        KeyCode::Enter => app.screen = Screen::Detail,
        KeyCode::Char('n') => start_input(app, InputMode::NewProfile),
        KeyCode::Char('i') => start_input(app, InputMode::ImportDefault),
        KeyCode::Char('d') => start_input(app, InputMode::DeleteConfirm),
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

fn handle_detail(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    key: KeyEvent,
) -> Result<bool> {
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Char('b') => app.screen = Screen::List,
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
        KeyCode::Char('r') => app.doctor_checks = crate::doctor::run(false)?,
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
            app.input_mode = InputMode::None;
        }
        KeyCode::Enter => submit_input(terminal, app)?,
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(true),
        KeyCode::Char(ch) => {
            if matches!(
                app.input_mode,
                InputMode::NewProfile
                    | InputMode::ImportDefault
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
        && !matches!(mode, InputMode::NewProfile | InputMode::ImportDefault)
    {
        app.set_message("No profile selected");
    } else {
        app.input_mode = mode;
    }
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
    super::suspend_terminal(terminal)?;
    let status = match action {
        External::Login => crate::process::codex_login(&name)?,
        External::Run => crate::process::codex_run(&name, &[])?,
    };
    wait_for_enter(status)?;
    super::resume_terminal(terminal)?;
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
    super::suspend_terminal(terminal)?;
    let status = crate::process::codex_exec(&name, &[prompt])?;
    wait_for_enter(status)?;
    super::resume_terminal(terminal)?;
    app.refresh_profiles()?;
    Ok(())
}

fn wait_for_enter(status: i32) -> Result<()> {
    println!();
    println!("codex exited with status {status}.");
    print!("Press Enter to return to CodexHub TUI...");
    io::stdout().flush().ok();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(())
}
