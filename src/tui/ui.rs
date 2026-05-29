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
        Screen::History => draw_history(frame, app),
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
        let name = if app.active_profile.as_deref() == Some(p.name.as_str()) {
            format!("* {}", p.name)
        } else {
            p.name.clone()
        };
        Row::new(vec![
            Cell::from(Span::styled(
                name,
                Style::default()
                    .fg(widgets::TEXT)
                    .add_modifier(Modifier::BOLD),
            )),
            Cell::from(login),
            Cell::from(p.plan_type.clone().unwrap_or_else(|| "-".into())),
            Cell::from(percent(p.limit_5h_remaining)),
            Cell::from(percent(p.limit_7day_remaining)),
            Cell::from(expiry(p.plan_expires_at)),
            Cell::from(Span::styled(auth, Style::default().fg(widgets::MUTED))),
            Cell::from(size::human(p.sessions_size)),
            Cell::from(size::human(p.logs_size)),
            Cell::from(size::human(p.total_size)),
            Cell::from(shared),
        ])
        .style(style)
    });
    let title = if let Some(active) = &app.active_profile {
        if app.status_loading {
            format!("CodexHub Profiles (active: {active}, loading status...)")
        } else {
            format!("CodexHub Profiles (active: {active})")
        }
    } else if app.status_loading {
        "CodexHub Profiles (loading status...)".to_string()
    } else {
        "CodexHub Profiles".to_string()
    };
    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Length(16),
            Constraint::Length(18),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new([
            "Name", "Login", "Plan", "5h", "7day", "Expires", "Auth Age", "Sessions", "Logs",
            "Total", "Shared",
        ])
        .style(widgets::header_style()),
    )
    .block(widgets::block(&title));
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
            (
                "Codex",
                &[
                    ("a", "activate"),
                    ("l", "login"),
                    ("r", "run"),
                    ("e", "exec"),
                ],
            ),
            ("History", &[("h", "resume")]),
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
            #[cfg(unix)]
            let auth_inode = std::fs::metadata(&auth)
                .ok()
                .map(|m| {
                    use std::os::unix::fs::MetadataExt;
                    m.ino().to_string()
                })
                .unwrap_or_else(|| "-".into());
            #[cfg(not(unix))]
            let auth_inode = "-".to_string();
            let active_env = crate::config::paths()
                .map(|paths| paths.root.join("current.env").display().to_string())
                .unwrap_or_else(|_| "-".into());
            vec![
                format!("Profile Name: {}", p.name),
                format!("Profile Path: {}", p.path.display()),
                format!(
                    "Active Profile: {}",
                    if app.active_profile.as_deref() == Some(name.as_str()) {
                        "yes"
                    } else {
                        "no"
                    }
                ),
                format!(
                    "Active Env: {}",
                    app.active_profile
                        .as_ref()
                        .map(|_| active_env.as_str())
                        .unwrap_or("-")
                ),
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
                format!("Plan: {}", p.plan_type.unwrap_or_else(|| "-".into())),
                format!("5h Remaining: {}", percent(p.limit_5h_remaining)),
                format!("7day Remaining: {}", percent(p.limit_7day_remaining)),
                format!("Plan Expires: {}", expiry(p.plan_expires_at)),
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
            (
                "Codex",
                &[
                    ("a", "activate"),
                    ("l", "login"),
                    ("r", "run"),
                    ("e", "exec"),
                ],
            ),
            ("Cache", &[("s", "share"), ("u", "unshare")]),
            ("System", &[("D", "doctor"), ("q", "quit")]),
        ]),
        chunks[1],
    );
}

fn percent(value: Option<u8>) -> String {
    value
        .map(|value| format!("{value}%"))
        .unwrap_or_else(|| "-".into())
}

fn expiry(value: Option<chrono::DateTime<chrono::Local>>) -> String {
    value
        .map(|value| value.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "-".into())
}

fn history_time(timestamp: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0)
        .map(chrono::DateTime::<chrono::Local>::from)
        .map(|value| value.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".into())
}

fn short_home(path: &str) -> String {
    let Some(home) = dirs::home_dir() else {
        return path.to_string();
    };
    let home = home.to_string_lossy();
    path.strip_prefix(home.as_ref())
        .map(|rest| format!("~{rest}"))
        .unwrap_or_else(|| path.to_string())
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

fn draw_history(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(4)])
        .split(area);
    let visible_rows = usize::from(chunks[0].height.saturating_sub(3)).max(1);
    let start = app
        .selected_history
        .saturating_add(1)
        .saturating_sub(visible_rows);
    let rows = app
        .history_sessions
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_rows)
        .map(|(idx, session)| {
            let style = if idx == app.selected_history {
                widgets::selected_style()
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(Span::styled(
                    session.profile.clone(),
                    Style::default().fg(widgets::TEXT),
                )),
                Cell::from(history_time(session.updated_at)),
                Cell::from(session.title.clone()),
                Cell::from(
                    session
                        .cwd
                        .as_deref()
                        .map(short_home)
                        .unwrap_or_else(|| "-".into()),
                ),
                Cell::from(Span::styled(
                    session.session_id.clone(),
                    Style::default().fg(widgets::MUTED),
                )),
            ])
            .style(style)
        });
    let table = Table::new(
        rows,
        [
            Constraint::Length(24),
            Constraint::Length(16),
            Constraint::Min(28),
            Constraint::Length(28),
            Constraint::Length(20),
        ],
    )
    .header(
        Row::new(["Profile", "Updated", "Title", "CWD", "Session"]).style(widgets::header_style()),
    )
    .block(widgets::block(if app.history_loading {
        "Resume Sessions (loading...)"
    } else {
        "Resume Sessions"
    }));
    frame.render_widget(table, chunks[0]);
    frame.render_widget(
        widgets::help_bar(&[
            ("Move", &[("↑↓", "select"), ("j/k", "select")]),
            (
                "Session",
                &[
                    ("Enter", "resume"),
                    ("c", "continue as"),
                    ("r", "refresh"),
                    ("b", "back"),
                ],
            ),
            ("System", &[("q", "quit")]),
        ]),
        chunks[1],
    );
}

fn draw_popup(frame: &mut Frame<'_>, app: &App) {
    if app.input_mode == InputMode::ContinueProfile {
        draw_continue_profile_popup(frame, app);
        return;
    }

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
        InputMode::ContinueProfile => return,
        InputMode::ExecPrompt => ("Codex Exec", format!("Prompt: {}", app.input)),
        InputMode::ShareConfirm => (
            "Share Cache",
            "Press Enter to confirm, Esc to cancel".into(),
        ),
        InputMode::UnshareConfirm => (
            "Unshare Cache",
            "Press Enter to confirm, Esc to cancel".into(),
        ),
        InputMode::UpdatePrompt => {
            let body = app
                .update_info
                .as_ref()
                .map(|info| {
                    format!(
                        "A CodexHub update is available.\n\nLocal:  {}\nRemote: {}\n\nPress Enter or y to update, n or Esc to skip.",
                        short_hash(&info.local_head),
                        short_hash(&info.remote_head)
                    )
                })
                .unwrap_or_else(|| "No update information available.".into());
            ("Update Available", body)
        }
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

fn draw_continue_profile_popup(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(58, 52, frame.area());
    frame.render_widget(Clear, area);
    let visible_rows = usize::from(area.height.saturating_sub(4)).max(1);
    let start = app
        .selected_continue_profile
        .saturating_add(1)
        .saturating_sub(visible_rows);
    let source = app
        .current_history_session()
        .map(|session| session.profile)
        .unwrap_or_default();
    let items = app
        .profiles
        .iter()
        .enumerate()
        .skip(start)
        .take(visible_rows)
        .map(|(idx, profile)| {
            let marker = if profile.name == source {
                " current session"
            } else if app.active_profile.as_deref() == Some(profile.name.as_str()) {
                " active"
            } else {
                ""
            };
            let style = if idx == app.selected_continue_profile {
                widgets::selected_style()
            } else {
                Style::default().fg(widgets::TEXT).bg(widgets::BG)
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    profile.name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(marker, Style::default().fg(widgets::MUTED)),
            ]))
            .style(style)
        });
    frame.render_widget(
        List::new(items)
            .block(widgets::block("Continue With Profile"))
            .style(Style::default().fg(widgets::TEXT).bg(widgets::BG)),
        area,
    );
}

fn short_hash(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
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
