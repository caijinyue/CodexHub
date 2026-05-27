use super::{
    app::App,
    screens::{InputMode, Screen},
    widgets,
};
use crate::size;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Clear, List, ListItem, Paragraph, Row, Table, Wrap},
};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    match app.screen {
        Screen::List => draw_list(frame, app),
        Screen::Detail => draw_detail(frame, app),
        Screen::Doctor => draw_doctor(frame, app),
    }
    if app.input_mode != InputMode::None {
        draw_popup(frame, app);
    }
}

fn draw_list(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(4)])
        .split(area);

    let rows = app.profiles.iter().enumerate().map(|(idx, p)| {
        let auth = p
            .auth_mtime
            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".into());
        let style = if idx == app.selected {
            widgets::selected_style()
        } else {
            Style::default()
        };
        let login = if p.logged_in {
            Span::styled(
                "yes",
                Style::default()
                    .fg(widgets::OK)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("no", Style::default().fg(widgets::WARN))
        };
        let shared = if p.shared_cache {
            Span::styled(
                "yes",
                Style::default()
                    .fg(widgets::CACHE)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("no", Style::default().fg(widgets::MUTED))
        };
        Row::new(vec![
            Cell::from(Span::styled(
                p.name.clone(),
                Style::default()
                    .fg(widgets::TEXT)
                    .add_modifier(Modifier::BOLD),
            )),
            Cell::from(login),
            Cell::from(Span::styled(auth, Style::default().fg(widgets::MUTED))),
            Cell::from(size::human(p.sessions_size)),
            Cell::from(size::human(p.logs_size)),
            Cell::from(size::human(p.total_size)),
            Cell::from(shared),
            Cell::from(Span::styled(
                p.path.display().to_string(),
                Style::default().fg(widgets::MUTED),
            )),
        ])
        .style(style)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(7),
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new([
            "Name", "Login", "Auth Age", "Sessions", "Logs", "Total", "Shared", "Path",
        ])
        .style(widgets::header_style()),
    )
    .block(widgets::block("CodexHub Profiles"));
    frame.render_widget(table, chunks[0]);
    frame.render_widget(
        widgets::help_bar(&[
            ("Move", &[("↑↓", "select"), ("j/k", "select")]),
            (
                "Profile",
                &[
                    ("Enter", "detail"),
                    ("n", "new"),
                    ("i", "import"),
                    ("2", "sub2"),
                    ("d", "delete"),
                ],
            ),
            ("Codex", &[("l", "login"), ("r", "run"), ("e", "exec")]),
            ("Cache", &[("s", "share"), ("u", "unshare")]),
            ("System", &[("D", "doctor"), ("q", "quit")]),
        ]),
        chunks[1],
    );
}

fn draw_detail(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let Some(name) = app.current_name() else {
        frame.render_widget(
            Paragraph::new("No profile selected").block(widgets::block("Profile Detail")),
            area,
        );
        return;
    };
    let lines = match crate::profile::metadata(&name) {
        Ok(p) => {
            let auth = p.path.join("auth.json");
            let auth_meta = std::fs::symlink_metadata(&auth).ok();
            let auth_inode = std::fs::metadata(&auth)
                .ok()
                .map(|m| {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        m.ino().to_string()
                    }
                    #[cfg(not(unix))]
                    {
                        "-".to_string()
                    }
                })
                .unwrap_or_else(|| "-".into());
            vec![
                format!("Profile Name: {}", p.name),
                format!("Profile Path: {}", p.path.display()),
                format!("Auth Exists: {}", auth.exists()),
                format!(
                    "Auth Mtime: {}",
                    p.auth_mtime
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_else(|| "-".into())
                ),
                format!(
                    "Auth Is Symlink: {}",
                    auth_meta
                        .map(|m| m.file_type().is_symlink())
                        .unwrap_or(false)
                ),
                format!("Auth Inode: {auth_inode}"),
                format!("Config Exists: {}", p.path.join("config.toml").exists()),
                format!(
                    "History Size: {}",
                    size::human(size::path_size(&p.path.join("history.jsonl")).unwrap_or(0))
                ),
                format!(
                    "Session Index Size: {}",
                    size::human(size::path_size(&p.path.join("session_index.jsonl")).unwrap_or(0))
                ),
                format!("Sessions Size: {}", size::human(p.sessions_size)),
                format!("Logs Size: {}", size::human(p.logs_size)),
                format!("Total Size: {}", size::human(p.total_size)),
                format!(
                    "Shared Cache Status: {}",
                    if p.shared_cache { "yes" } else { "no" }
                ),
                format!("Broken Symlinks: {}", broken_symlinks(&p.path).join(", ")),
            ]
        }
        Err(err) => vec![format!("Error: {err}")],
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(4)])
        .split(area);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .block(widgets::block("Profile Detail"))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );
    frame.render_widget(
        widgets::help_bar(&[
            ("Move", &[("b", "back")]),
            ("Codex", &[("l", "login"), ("r", "run"), ("e", "exec")]),
            ("Cache", &[("s", "share"), ("u", "unshare")]),
            ("System", &[("D", "doctor"), ("q", "quit")]),
        ]),
        chunks[1],
    );
}

fn draw_doctor(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let items = app.doctor_checks.iter().map(|c| {
        let color = match c.level {
            crate::doctor::Level::Ok => widgets::OK,
            crate::doctor::Level::Warn => widgets::WARN,
            crate::doctor::Level::Error => widgets::ERROR,
        };
        ListItem::new(Line::from(vec![
            Span::styled(
                format!("{:<5}", c.level.as_str()),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}: ", c.subject),
                Style::default().fg(widgets::TITLE),
            ),
            Span::styled(c.message.clone(), Style::default().fg(widgets::TEXT)),
        ]))
    });
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(4)])
        .split(area);
    frame.render_widget(List::new(items).block(widgets::block("Doctor")), chunks[0]);
    frame.render_widget(
        widgets::help_bar(&[
            ("Move", &[("b", "back")]),
            ("System", &[("r", "rerun"), ("q", "quit")]),
        ]),
        chunks[1],
    );
}

fn draw_popup(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(70, 30, frame.area());
    frame.render_widget(Clear, area);
    let (title, body) = match app.input_mode {
        InputMode::NewProfile => ("New Profile", format!("Name: {}", app.input)),
        InputMode::ImportDefault => (
            "Import ~/.codex",
            format!(
                "Profile name: {}\nLeave empty to use the email address from ~/.codex/auth.json.",
                app.input
            ),
        ),
        InputMode::ImportSub2 => (
            "Import sub2 JSON",
            format!(
                "JSON path: {}\nCreates a profile named from the account email.",
                app.input
            ),
        ),
        InputMode::DeleteConfirm => {
            let expected = app.current_name().unwrap_or_default();
            (
                "Delete Profile",
                format!("Type \"{expected}\" to confirm:\n{}", app.input),
            )
        }
        InputMode::ExecPrompt => ("Codex Exec", format!("Prompt: {}", app.input)),
        InputMode::ShareConfirm => (
            "Share Cache",
            "Press Enter to confirm, Esc to cancel".into(),
        ),
        InputMode::UnshareConfirm => (
            "Unshare Cache",
            "Press Enter to confirm, Esc to cancel".into(),
        ),
        InputMode::Message => ("Message", app.message.clone()),
        InputMode::None => return,
    };
    frame.render_widget(
        Paragraph::new(body)
            .block(widgets::block(title))
            .style(Style::default().fg(widgets::TEXT).bg(widgets::BG))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn broken_symlinks(path: &std::path::Path) -> Vec<String> {
    walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            if crate::config::is_broken_symlink(e.path()) {
                Some(e.path().display().to_string())
            } else {
                None
            }
        })
        .collect()
}
