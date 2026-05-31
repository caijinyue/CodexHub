use super::{
    app::App,
    screens::{InputMode, Screen},
    widgets,
};
use crate::{profile::ProfileInfo, size};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Gauge, List, ListItem, Paragraph, Row, Table, Wrap},
};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    match app.screen {
        Screen::List => draw_profile_workspace(frame, app),
        Screen::Detail => draw_profile_workspace(frame, app),
        Screen::Doctor => draw_doctor(frame, app),
        Screen::History => draw_history(frame, app),
    }
    if app.input_mode != InputMode::None {
        draw_popup(frame, app);
    }
}

fn draw_profile_workspace(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(area);
    let body = if vertical[0].width < 88 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(12), Constraint::Min(12)])
            .split(vertical[0])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(32), Constraint::Min(48)])
            .split(vertical[0])
    };

    draw_profile_sidebar(frame, app, body[0]);
    draw_profile_main(frame, app, body[1]);
    draw_footer(
        frame,
        app,
        vertical[1],
        &[
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
            (
                "System",
                &[
                    ("h", "history"),
                    ("t", "theme"),
                    ("D", "doctor"),
                    ("q", "quit"),
                ],
            ),
        ],
    );
}

fn draw_profile_sidebar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(5)])
        .split(area);
    let active = app.active_profile.as_deref().unwrap_or("-");
    let summary = vec![
        Line::from(vec![
            Span::styled("Active ", Style::default().fg(app.theme.muted)),
            Span::styled(
                active.to_string(),
                Style::default()
                    .fg(app.theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Theme  ", Style::default().fg(app.theme.muted)),
            Span::styled(app.theme_label(), Style::default().fg(app.theme.text)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(summary)
            .block(widgets::block("CodexHub", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.surface)),
        inner[0],
    );

    let visible = usize::from(inner[1].height.saturating_sub(2)).max(1);
    let start = app.selected.saturating_add(1).saturating_sub(visible);
    let items = app
        .profiles
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(idx, profile)| {
            let selected = idx == app.selected;
            let active = app.active_profile.as_deref() == Some(profile.name.as_str());
            let status = if profile.logged_in {
                "signed in"
            } else {
                "not signed in"
            };
            let quota = format!(
                "5h {}  7d {}",
                percent(profile.limit_5h_remaining),
                percent(profile.limit_7day_remaining)
            );
            let title = if active {
                format!("▌ {}  ACTIVE", profile.name)
            } else {
                format!("  {}", profile.name)
            };
            let style = if selected {
                widgets::selected_style(app.theme)
            } else {
                Style::default().fg(app.theme.text).bg(app.theme.surface)
            };
            ListItem::new(vec![
                Line::from(Span::styled(
                    title,
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(vec![
                    Span::styled(
                        status,
                        Style::default().fg(if profile.logged_in {
                            app.theme.ok
                        } else {
                            app.theme.warn
                        }),
                    ),
                    Span::styled("  ", Style::default()),
                    Span::styled(quota, Style::default().fg(app.theme.muted)),
                ]),
            ])
            .style(style)
        });
    frame.render_widget(
        List::new(items)
            .block(widgets::block("Profiles", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.surface)),
        inner[1],
    );
}

fn draw_profile_main(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(profile) = app.profiles.get(app.selected) else {
        frame.render_widget(
            Paragraph::new("No profile selected")
                .block(widgets::block("Profile", app.theme))
                .style(Style::default().fg(app.theme.text).bg(app.theme.panel)),
            area,
        );
        return;
    };

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Min(8),
        ])
        .split(area);
    draw_profile_header(frame, app, profile, sections[0]);
    draw_quota_panel(frame, app, profile, sections[1]);
    draw_profile_details(frame, app, profile, sections[2]);
}

fn draw_profile_header(frame: &mut Frame<'_>, app: &App, profile: &ProfileInfo, area: Rect) {
    let active = app.active_profile.as_deref() == Some(profile.name.as_str());
    let status = if profile.logged_in {
        "SIGNED IN"
    } else {
        "NOT SIGNED IN"
    };
    let plan = profile.plan_type.clone().unwrap_or_else(|| "-".into());
    let lines = vec![
        Line::from(vec![
            Span::styled(
                profile.name.clone(),
                Style::default()
                    .fg(app.theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if active { "  ACTIVE" } else { "" },
                Style::default()
                    .fg(app.theme.ok)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                status,
                Style::default()
                    .fg(if profile.logged_in {
                        app.theme.ok
                    } else {
                        app.theme.warn
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Plan ", Style::default().fg(app.theme.muted)),
            Span::styled(plan, Style::default().fg(app.theme.text)),
            Span::styled("  Member ", Style::default().fg(app.theme.muted)),
            Span::styled(
                expiry(profile.plan_expires_at),
                Style::default().fg(app.theme.text),
            ),
        ]),
        Line::from(Span::styled(
            profile.path.display().to_string(),
            Style::default().fg(app.theme.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(widgets::block("Selected Profile", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.panel)),
        area,
    );
}

fn draw_quota_panel(frame: &mut Frame<'_>, app: &App, profile: &ProfileInfo, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title("Quota")
            .border_style(Style::default().fg(app.theme.border))
            .style(Style::default().bg(app.theme.panel)),
        area,
    );
    draw_gauge(frame, app, rows[0], "5h", profile.limit_5h_remaining);
    draw_gauge(frame, app, rows[1], "7day", profile.limit_7day_remaining);
}

fn draw_gauge(frame: &mut Frame<'_>, app: &App, area: Rect, label: &str, value: Option<u8>) {
    let value = value.unwrap_or(0).min(100);
    let gauge_area = Rect {
        x: area.x.saturating_add(2),
        y: area.y,
        width: area.width.saturating_sub(4),
        height: area.height,
    };
    frame.render_widget(
        Gauge::default()
            .label(format!("{label} remaining {value}%"))
            .ratio(f64::from(value) / 100.0)
            .gauge_style(
                Style::default()
                    .fg(quota_color(app, value))
                    .bg(app.theme.surface)
                    .add_modifier(Modifier::BOLD),
            ),
        gauge_area,
    );
}

fn draw_profile_details(frame: &mut Frame<'_>, app: &App, profile: &ProfileInfo, area: Rect) {
    let auth = profile.path.join("auth.json");
    let config = profile.path.join("config.toml");
    let active_env = crate::config::paths()
        .map(|paths| paths.root.join("current.env").display().to_string())
        .unwrap_or_else(|_| "-".into());
    let rows = vec![
        detail_row("Auth exists", bool_label(auth.exists())),
        detail_row(
            "Auth mtime",
            profile
                .auth_mtime
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "-".into()),
        ),
        detail_row("Config exists", bool_label(config.exists())),
        detail_row(
            "Active env",
            if app.active_profile.is_some() {
                active_env
            } else {
                "-".into()
            },
        ),
        detail_row("Sessions", size::human(profile.sessions_size)),
        detail_row("Logs", size::human(profile.logs_size)),
        detail_row("Total", size::human(profile.total_size)),
        detail_row("Shared cache", bool_label(profile.shared_cache)),
    ];
    let table = Table::new(rows, [Constraint::Length(16), Constraint::Min(20)])
        .block(widgets::block("Details", app.theme))
        .style(Style::default().fg(app.theme.text).bg(app.theme.panel))
        .column_spacing(2);
    frame.render_widget(table, area);
}

fn draw_doctor(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(3)])
        .split(area);
    let items = app.doctor_checks.iter().map(|c| {
        let color = match c.level {
            crate::doctor::Level::Ok => app.theme.ok,
            crate::doctor::Level::Warn => app.theme.warn,
            crate::doctor::Level::Error => app.theme.error,
        };
        ListItem::new(Line::from(vec![
            Span::styled(
                format!("{:<5}", c.level.as_str()),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {}: ", c.subject),
                Style::default().fg(app.theme.title),
            ),
            Span::styled(c.message.clone(), Style::default().fg(app.theme.text)),
        ]))
    });
    frame.render_widget(
        List::new(items)
            .block(widgets::block("Doctor", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.panel)),
        chunks[0],
    );
    draw_footer(
        frame,
        app,
        chunks[1],
        &[
            ("Move", &[("b", "back")]),
            ("System", &[("t", "theme"), ("r", "rerun"), ("q", "quit")]),
        ],
    );
}

fn draw_history(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(area);
    let body = if vertical[0].width < 92 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(vertical[0])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(vertical[0])
    };
    draw_session_list(frame, app, body[0]);
    draw_session_detail(frame, app, body[1]);
    draw_footer(
        frame,
        app,
        vertical[1],
        &[
            ("Move", &[("↑↓", "select"), ("j/k", "select")]),
            (
                "Session",
                &[
                    ("Enter", "resume"),
                    ("c", "continue as"),
                    ("a", "all/current"),
                    ("r", "refresh"),
                    ("b", "back"),
                ],
            ),
            ("System", &[("t", "theme"), ("q", "quit")]),
        ],
    );
}

fn draw_session_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    let start = app
        .selected_history
        .saturating_add(1)
        .saturating_sub(visible);
    let items = app
        .history_sessions
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(idx, session)| {
            let selected = idx == app.selected_history;
            let style = if selected {
                widgets::selected_style(app.theme)
            } else {
                Style::default().fg(app.theme.text).bg(app.theme.surface)
            };
            ListItem::new(vec![
                Line::from(Span::styled(
                    session.title.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(vec![
                    Span::styled(
                        session.profile.clone(),
                        Style::default().fg(app.theme.title),
                    ),
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        history_time(session.updated_at),
                        Style::default().fg(app.theme.muted),
                    ),
                ]),
            ])
            .style(style)
        });
    let title = if app.history_loading {
        format!("Sessions: {} (loading...)", app.history_scope_label())
    } else {
        format!("Sessions: {}", app.history_scope_label())
    };
    frame.render_widget(
        List::new(items)
            .block(widgets::block(&title, app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.surface)),
        area,
    );
}

fn draw_session_detail(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(session) = app.current_history_session() else {
        frame.render_widget(
            Paragraph::new("No session selected")
                .block(widgets::block("Session Detail", app.theme))
                .style(Style::default().fg(app.theme.text).bg(app.theme.panel)),
            area,
        );
        return;
    };
    let updated = history_time(session.updated_at);
    let cwd = session
        .cwd
        .as_deref()
        .map(short_home)
        .unwrap_or_else(|| "-".into());
    let path = session
        .path
        .as_deref()
        .map(short_home)
        .unwrap_or_else(|| "-".into());
    let lines = vec![
        Line::from(Span::styled(
            session.title,
            Style::default()
                .fg(app.theme.title)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        detail_line(app, "Profile", session.profile),
        detail_line(app, "Updated", updated),
        detail_line(app, "CWD", cwd),
        detail_line(app, "Session", session.session_id),
        detail_line(app, "Path", path),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Enter",
                Style::default()
                    .fg(app.theme.key_fg)
                    .bg(app.theme.title)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" resume   ", Style::default().fg(app.theme.text)),
            Span::styled(
                "c",
                Style::default()
                    .fg(app.theme.key_fg)
                    .bg(app.theme.action)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " continue with another profile",
                Style::default().fg(app.theme.text),
            ),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(widgets::block("Session Detail", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.panel))
            .wrap(Wrap { trim: false }),
        area,
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
            .block(widgets::block(title, app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.bg))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_continue_profile_popup(frame: &mut Frame<'_>, app: &App) {
    let area = centered_rect(64, 58, frame.area());
    frame.render_widget(Clear, area);
    let visible = usize::from(area.height.saturating_sub(3)).max(1);
    let start = app
        .selected_continue_profile
        .saturating_add(1)
        .saturating_sub(visible);
    let source = app
        .current_history_session()
        .map(|session| session.profile)
        .unwrap_or_default();
    let rows = app
        .profiles
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(idx, profile)| {
            let marker = if profile.name == source {
                "current"
            } else if app.active_profile.as_deref() == Some(profile.name.as_str()) {
                "active"
            } else {
                ""
            };
            let style = if idx == app.selected_continue_profile {
                widgets::selected_style(app.theme)
            } else {
                Style::default().fg(app.theme.text).bg(app.theme.panel)
            };
            Row::new(vec![
                Cell::from(profile.name.clone()),
                Cell::from(marker),
                Cell::from(percent(profile.limit_5h_remaining)),
                Cell::from(percent(profile.limit_7day_remaining)),
                Cell::from(expiry(profile.plan_expires_at)),
            ])
            .style(style)
        });
    let table = Table::new(
        rows,
        [
            Constraint::Min(18),
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(12),
        ],
    )
    .header(
        Row::new(["Profile", "State", "5h", "7day", "Member"])
            .style(widgets::header_style(app.theme)),
    )
    .block(widgets::block("Continue With Profile", app.theme))
    .style(Style::default().fg(app.theme.text).bg(app.theme.panel));
    frame.render_widget(table, area);
}

fn draw_footer(frame: &mut Frame<'_>, app: &App, area: Rect, groups: &[(&str, &[(&str, &str)])]) {
    frame.render_widget(widgets::help_bar(groups, app.theme), area);
}

fn detail_row(label: impl Into<String>, value: impl Into<String>) -> Row<'static> {
    Row::new(vec![Cell::from(label.into()), Cell::from(value.into())])
}

fn detail_line(app: &App, label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<10}"), Style::default().fg(app.theme.muted)),
        Span::styled(value.into(), Style::default().fg(app.theme.text)),
    ])
}

fn quota_color(app: &App, value: u8) -> ratatui::style::Color {
    if value <= 10 {
        app.theme.error
    } else if value <= 30 {
        app.theme.warn
    } else {
        app.theme.ok
    }
}

fn bool_label(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
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
