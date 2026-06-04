use super::{
    app::App,
    screens::{InputMode, Screen},
    widgets,
};
use crate::{profile::ProfileInfo, size};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Frame,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Gauge, List, ListItem, Paragraph, Row, Table, Wrap},
};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    match app.screen {
        Screen::List => draw_profile_workspace(frame, app),
        Screen::Doctor => draw_doctor(frame, app),
        Screen::History => draw_history(frame, app),
        Screen::Remote => draw_remote(frame, app),
        Screen::SharedAccounts => draw_shared_accounts(frame, app),
        Screen::Settings => draw_settings(frame, app),
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
            (
                "Account",
                &[
                    ("Enter", "activate"),
                    ("n", "add"),
                    ("l", "relogin"),
                    ("d", "delete"),
                ],
            ),
            ("Codex", &[("o", "open")]),
            (
                "System",
                &[
                    ("h", "history"),
                    ("R", "remote"),
                    ("S", "shared"),
                    ("r", "refresh"),
                    ("s", "settings"),
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
        Line::from(vec![
            Span::styled("Update ", Style::default().fg(app.theme.muted)),
            update_status_span(app),
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
            let status = if active {
                "🟢 active"
            } else if profile.logged_in {
                "✅ signed in"
            } else {
                "🔐 relogin needed"
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
            .block(widgets::block("👤 Accounts", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.surface)),
        inner[1],
    );
}

fn draw_profile_main(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(profile) = app.profiles.get(app.selected) else {
        frame.render_widget(
            Paragraph::new("No accounts yet\n\nPress n to add your first Codex account.")
                .block(widgets::block("👤 Account", app.theme))
                .style(Style::default().fg(app.theme.text).bg(app.theme.panel)),
            area,
        );
        return;
    };

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(9),
            Constraint::Min(8),
        ])
        .split(area);
    draw_profile_header(frame, app, profile, sections[0]);
    draw_quota_panel(frame, app, profile, sections[1]);
    draw_profile_details(frame, app, profile, sections[2]);
}

fn draw_profile_header(frame: &mut Frame<'_>, app: &App, profile: &ProfileInfo, area: Rect) {
    let active = app.active_profile.as_deref() == Some(profile.name.as_str());
    let status = if active {
        "🟢 ACTIVE"
    } else if profile.logged_in {
        "✅ SIGNED IN"
    } else {
        "🔐 LOGIN NEEDED"
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
            Span::styled("", Style::default()),
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
            Span::styled("  Used ", Style::default().fg(app.theme.muted)),
            Span::styled(
                used_days(profile.used_since),
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
            .block(widgets::block("👤 Selected Account", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.panel)),
        area,
    );
}

fn draw_quota_panel(frame: &mut Frame<'_>, app: &App, profile: &ProfileInfo, area: Rect) {
    let inner = Rect {
        x: area.x.saturating_add(2),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3)])
        .split(inner);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(if app.status_loading {
                "📊 Quota refreshing"
            } else {
                "📊 Quota"
            })
            .border_style(Style::default().fg(app.theme.border))
            .style(Style::default().bg(app.theme.panel)),
        area,
    );
    draw_gauge(
        frame,
        app,
        rows[0],
        "5h",
        profile.limit_5h_remaining,
        profile.limit_5h_resets_at,
    );
    draw_gauge(
        frame,
        app,
        rows[1],
        "7day",
        profile.limit_7day_remaining,
        profile.limit_7day_resets_at,
    );
}

fn draw_gauge(
    frame: &mut Frame<'_>,
    app: &App,
    area: Rect,
    label: &str,
    value: Option<u8>,
    resets_at: Option<chrono::DateTime<chrono::Local>>,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);
    let value_text = value.map(|value| format!("{value}%")).unwrap_or_else(|| {
        if app.status_loading {
            "loading".into()
        } else {
            "-".into()
        }
    });
    let reset_text = resets_at
        .map(|value| format!("resets {}", reset_time(Some(value))))
        .unwrap_or_else(|| "resets -".into());
    let title = Line::from(vec![
        Span::styled(
            format!("{label:<5}"),
            Style::default()
                .fg(app.theme.title)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            value_text,
            Style::default()
                .fg(app.theme.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(reset_text, Style::default().fg(app.theme.muted)),
    ]);
    frame.render_widget(Paragraph::new(title), rows[0]);
    let value = value.map(|value| value.min(100));
    let ratio = value.map(|value| f64::from(value) / 100.0).unwrap_or(0.0);
    let color = value
        .map(|value| quota_color(app, value))
        .unwrap_or(app.theme.muted);
    frame.render_widget(
        Gauge::default().label("").ratio(ratio).gauge_style(
            Style::default()
                .fg(color)
                .bg(app.theme.surface)
                .add_modifier(Modifier::BOLD),
        ),
        rows[1],
    );
}

fn draw_profile_details(frame: &mut Frame<'_>, app: &App, profile: &ProfileInfo, area: Rect) {
    let active_env = crate::config::paths()
        .map(|paths| paths.root.join("current.env").display().to_string())
        .unwrap_or_else(|_| "-".into());
    let rows = vec![
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
        detail_row("Used", used_days(profile.used_since)),
        detail_row("Member", expiry(profile.plan_expires_at)),
        detail_row("Update check", update_detail_status(app)),
        detail_row(
            "Auth updated",
            profile
                .auth_mtime
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "-".into()),
        ),
        detail_row("Path", profile.path.display().to_string()),
    ];
    let table = Table::new(rows, [Constraint::Length(16), Constraint::Min(20)])
        .block(widgets::block("💾 Storage", app.theme))
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
            .block(widgets::block("🩺 Doctor", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.panel)),
        chunks[0],
    );
    draw_footer(
        frame,
        app,
        chunks[1],
        &[("System", &[("r", "rerun"), ("t", "theme"), ("q", "back")])],
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
            (
                "Session",
                &[
                    ("Enter", "resume"),
                    ("c", "continue as"),
                    ("d", "delete"),
                    ("a", "all/current"),
                ],
            ),
            ("System", &[("r", "refresh"), ("t", "theme"), ("q", "back")]),
        ],
    );
}

fn draw_settings(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(3)])
        .split(area);
    let rows = vec![
        detail_row("Theme", app.theme_label()),
        detail_row("Quota refresh", app.quota_refresh_label()),
        detail_row(
            "Refresh state",
            if app.status_loading {
                "refreshing".to_string()
            } else {
                "idle".to_string()
            },
        ),
    ];
    let table = Table::new(rows, [Constraint::Length(18), Constraint::Min(24)])
        .block(widgets::block("⚙ Settings", app.theme))
        .style(Style::default().fg(app.theme.text).bg(app.theme.panel))
        .column_spacing(2);
    frame.render_widget(table, chunks[0]);
    draw_footer(
        frame,
        app,
        chunks[1],
        &[
            ("Refresh", &[("+/-", "interval"), ("r", "refresh now")]),
            ("System", &[("t", "theme"), ("q", "back")]),
        ],
    );
}

fn draw_remote(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(12), Constraint::Length(3)])
        .split(area);
    let status = crate::remote::status();
    let rows = match status {
        Ok(status) => vec![
            detail_row(
                "Password",
                if status.password_configured {
                    "configured"
                } else {
                    "not configured"
                },
            ),
            detail_row("Config", status.config_path.display().to_string()),
            detail_row("Local URL", status.localhost_url),
            detail_row("Tailscale URL", status.tailscale_url),
            detail_row("Local command", status.localhost_command),
            detail_row("Tailscale cmd", status.tailscale_command),
            detail_row("Password cmd", "codexhub remote password <new-password>"),
        ],
        Err(err) => vec![detail_row("Remote", format!("unavailable: {err}"))],
    };
    let table = Table::new(rows, [Constraint::Length(14), Constraint::Min(28)])
        .block(widgets::block("🌐 Remote", app.theme))
        .style(Style::default().fg(app.theme.text).bg(app.theme.panel))
        .column_spacing(2);
    frame.render_widget(table, chunks[0]);
    draw_footer(
        frame,
        app,
        chunks[1],
        &[("System", &[("t", "theme"), ("q", "back")])],
    );
}

fn draw_shared_accounts(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(area);
    let body = if vertical[0].width < 92 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(vertical[0])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(vertical[0])
    };
    draw_share_profile_list(frame, app, body[0]);
    draw_share_detail(frame, app, body[1]);
    draw_footer(
        frame,
        app,
        vertical[1],
        &[
            (
                "Share",
                &[("S", "share"), ("u", "unshare"), ("i", "import")],
            ),
            ("System", &[("r", "refresh"), ("t", "theme"), ("q", "back")]),
        ],
    );
}

fn draw_share_profile_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let visible = usize::from(area.height.saturating_sub(2)).max(1);
    let start = app.selected.saturating_add(1).saturating_sub(visible);
    let items = app
        .profiles
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(idx, profile)| {
            let selected = idx == app.selected;
            let exported = app.shared_account_for_profile(&profile.name).is_some();
            let imported = imported_shared_source(profile)
                .map(|source| source.display().to_string())
                .is_some();
            let state = match (exported, imported) {
                (true, true) => "shared + imported",
                (true, false) => "shared",
                (false, true) => "imported",
                (false, false) => "local only",
            };
            let style = if selected {
                widgets::selected_style(app.theme)
            } else {
                Style::default().fg(app.theme.text).bg(app.theme.surface)
            };
            ListItem::new(vec![
                Line::from(Span::styled(
                    profile.name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(state, Style::default().fg(app.theme.muted))),
            ])
            .style(style)
        });
    frame.render_widget(
        List::new(items)
            .block(widgets::block("🔗 Accounts", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.surface)),
        area,
    );
}

fn draw_share_detail(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(profile) = app.profiles.get(app.selected) else {
        frame.render_widget(
            Paragraph::new("No account selected")
                .block(widgets::block("🔗 Shared Account", app.theme))
                .style(Style::default().fg(app.theme.text).bg(app.theme.panel)),
            area,
        );
        return;
    };
    let exported = app.shared_account_for_profile(&profile.name);
    let imported = imported_shared_source(profile);
    let shared_root = crate::shared_account::SHARED_ACCOUNTS_ROOT.to_string();
    let rows = vec![
        detail_row("Account", profile.name.clone()),
        detail_row(
            "Exported",
            exported
                .map(|_| "yes".to_string())
                .unwrap_or_else(|| "no".into()),
        ),
        detail_row(
            "Imported",
            imported
                .as_ref()
                .map(|path| short_home(&path.display().to_string()))
                .unwrap_or_else(|| "no".into()),
        ),
        detail_row(
            "Owner",
            exported
                .and_then(|account| account.owner.clone())
                .unwrap_or_else(|| "-".into()),
        ),
        detail_row(
            "Allowed",
            exported
                .map(|account| {
                    if account.allowed_users.is_empty() {
                        "-".into()
                    } else {
                        account.allowed_users.join(", ")
                    }
                })
                .unwrap_or_else(|| "-".into()),
        ),
        detail_row(
            "Created",
            exported
                .and_then(|account| account.created_at)
                .map(|value| value.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "-".into()),
        ),
        detail_row(
            "Shared path",
            exported
                .map(|account| short_home(&account.path.display().to_string()))
                .unwrap_or_else(|| "-".into()),
        ),
        detail_row("Root", shared_root),
        detail_row("Local path", profile.path.display().to_string()),
    ];
    let table = Table::new(rows, [Constraint::Length(14), Constraint::Min(24)])
        .block(widgets::block("🔗 Shared Account", app.theme))
        .style(Style::default().fg(app.theme.text).bg(app.theme.panel))
        .column_spacing(2);
    frame.render_widget(table, area);
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
        format!("🕘 Sessions: {} (loading...)", app.history_scope_label())
    } else {
        format!("🕘 Sessions: {}", app.history_scope_label())
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
    let preview = crate::process::session_preview_messages(&session, 18);
    let mut lines = vec![
        Line::from(Span::styled(
            session.title.clone(),
            Style::default()
                .fg(app.theme.title)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        detail_line(app, "Source", session.profile),
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
                " continue with another account",
                Style::default().fg(app.theme.text),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "💬 Preview",
            Style::default()
                .fg(app.theme.title)
                .add_modifier(Modifier::BOLD),
        )),
    ];
    lines.extend(
        preview
            .into_iter()
            .map(|message| preview_message_line(app, message)),
    );
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
        InputMode::AddAccountMethod => (
            "➕ Add Account",
            "1  Login new account\n2  Use current ~/.codex\n3  Import JSON\n4  Import shared account\n\nEsc cancel".into(),
        ),
        InputMode::LoginMethodForNewAccount => (
            "Login Method",
            "1  Device code\n2  Web login\n\nEsc cancel".into(),
        ),
        InputMode::LoginMethodForSelected => (
            "Relogin Account",
            "1  Device code\n2  Web login\n\nEsc cancel".into(),
        ),
        InputMode::NewLoginProfileName => {
            ("➕ Add Account", format!("Account name: {}", app.input))
        }
        InputMode::ImportDefault => (
            "Use Current ~/.codex",
            format!(
                "Account name: {}\nLeave empty to use the email address from ~/.codex/auth.json.",
                app.input
            ),
        ),
        InputMode::ImportSub2 => (
            "Import JSON",
            format!(
                "JSON path: {}\nCreates an account named from the JSON email.",
                app.input
            ),
        ),
        InputMode::ShareTargetUser => (
            "Share Account",
            format!(
                "Target Linux user: {}\nShares only auth.json and config.toml.\nSessions and history stay private.",
                app.input
            ),
        ),
        InputMode::ShareNeedsSudo => {
            let body = app
                .pending_share_plan
                .as_ref()
                .map(|plan| {
                    format!(
                        "Share Account\n\nAccount: {}\nTarget user: {}\n\nNeeds sudo setup.\nPress Enter to open tmux sudo helper.\nEsc cancel.",
                        plan.profile, plan.target_user
                    )
                })
                .unwrap_or_else(|| "No pending shared account setup.".into());
            ("Share Account", body)
        }
        InputMode::RemoveSharedAccountConfirm => {
            let expected = app.current_name().unwrap_or_default();
            (
                "Remove Shared Account",
                format!(
                    "Type \"{expected}\" to remove this shared account.\nThis removes the shared copy only. The local account remains untouched.\n\n{}",
                    app.input
                ),
            )
        }
        InputMode::ImportSharedAccount => return draw_import_shared_account_popup(frame, app, area),
        InputMode::DeleteConfirm => {
            let expected = app.current_name().unwrap_or_default();
            (
                "Delete Account",
                format!(
                    "Type \"{expected}\" to delete this account login.\nSessions and history are preserved.\nThis does not delete your OpenAI account.\n\n{}",
                    app.input
                ),
            )
        }
        InputMode::DeleteSessionConfirm => {
            let session_id = app
                .current_history_session()
                .map(|session| session.session_id)
                .unwrap_or_default();
            (
                "Delete Session",
                format!(
                    "Type \"{session_id}\" to delete this history session.\nThis removes the selected rollout file and local index entries.\n\n{}",
                    app.input
                ),
            )
        }
        InputMode::ContinueProfile => return,
        InputMode::UpdatePrompt => return draw_update_popup(frame, app, area),
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

fn draw_update_popup(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let body = app
        .update_info
        .as_ref()
        .map(|info| {
            vec![
                Line::from(Span::styled(
                    "CodexHub update available",
                    Style::default()
                        .fg(app.theme.title)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                update_field_line(app, "Track", info.remote_ref.clone()),
                update_field_line(app, "Local", short_hash(&info.local_head).to_string()),
                update_field_line(app, "Remote", short_hash(&info.remote_head).to_string()),
                Line::from(""),
                Line::from(vec![
                    key_span(app, "Enter"),
                    Span::styled(" update   ", Style::default().fg(app.theme.text)),
                    key_span(app, "y"),
                    Span::styled(" update   ", Style::default().fg(app.theme.text)),
                    key_span(app, "n"),
                    Span::styled(" skip   ", Style::default().fg(app.theme.text)),
                    key_span(app, "Esc"),
                    Span::styled(" skip", Style::default().fg(app.theme.text)),
                ]),
            ]
        })
        .unwrap_or_else(|| {
            vec![Line::from(Span::styled(
                "No update information available.",
                Style::default().fg(app.theme.text),
            ))]
        });
    frame.render_widget(
        Paragraph::new(body)
            .block(widgets::block("⬆ Update", app.theme))
            .style(Style::default().fg(app.theme.text).bg(app.theme.panel))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn update_field_line(app: &App, label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<8}"),
            Style::default()
                .fg(app.theme.muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value, Style::default().fg(app.theme.text)),
    ])
}

fn key_span(app: &App, key: &str) -> Span<'static> {
    Span::styled(
        key.to_string(),
        Style::default()
            .fg(app.theme.key_fg)
            .bg(app.theme.action)
            .add_modifier(Modifier::BOLD),
    )
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
        Row::new(["Account", "State", "5h", "7day", "Member"])
            .style(widgets::header_style(app.theme)),
    )
    .block(widgets::block("Continue With Account", app.theme))
    .style(Style::default().fg(app.theme.text).bg(app.theme.panel));
    frame.render_widget(table, area);
}

fn draw_import_shared_account_popup(frame: &mut Frame<'_>, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let rows = app
        .shared_accounts
        .iter()
        .enumerate()
        .map(|(idx, account)| {
            let style = if idx == app.selected_shared_account {
                widgets::selected_style(app.theme)
            } else {
                Style::default().fg(app.theme.text).bg(app.theme.panel)
            };
            Row::new(vec![
                Cell::from(account.name.clone()),
                Cell::from(account.owner.clone().unwrap_or_else(|| "-".into())),
                Cell::from(account.allowed_users.join(",")),
                Cell::from(
                    account
                        .created_at
                        .map(|value| value.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| "-".into()),
                ),
                Cell::from(short_home(&account.path.display().to_string())),
            ])
            .style(style)
        });
    let table = Table::new(
        rows,
        [
            Constraint::Min(18),
            Constraint::Length(14),
            Constraint::Min(16),
            Constraint::Length(10),
            Constraint::Min(18),
        ],
    )
    .header(
        Row::new(["Account", "Owner", "Allowed", "Created", "Path"])
            .style(widgets::header_style(app.theme)),
    )
    .block(widgets::block("Import Shared Account", app.theme))
    .style(Style::default().fg(app.theme.text).bg(app.theme.panel));
    frame.render_widget(table, area);
    if app.shared_accounts.is_empty() {
        frame.render_widget(
            Paragraph::new("No shared accounts found.\n\nEsc cancel")
                .block(widgets::block("Import Shared Account", app.theme))
                .style(Style::default().fg(app.theme.text).bg(app.theme.panel)),
            area,
        );
    }
}

fn draw_footer(frame: &mut Frame<'_>, app: &App, area: Rect, groups: &[(&str, &[(&str, &str)])]) {
    frame.render_widget(widgets::help_bar(groups, app.theme), area);
}

fn update_status_span(app: &App) -> Span<'static> {
    if app.update_checking {
        Span::styled("checking...", Style::default().fg(app.theme.muted))
    } else if app.update_info.is_some() {
        Span::styled("available", Style::default().fg(app.theme.action))
    } else if app.update_error.is_some() {
        Span::styled("check failed", Style::default().fg(app.theme.warn))
    } else {
        Span::styled("current", Style::default().fg(app.theme.ok))
    }
}

fn update_detail_status(app: &App) -> String {
    if app.update_checking {
        "checking...".into()
    } else if let Some(info) = &app.update_info {
        format!(
            "available on {} ({} -> {})",
            info.remote_ref,
            short_hash(&info.local_head),
            short_hash(&info.remote_head)
        )
    } else if let Some(error) = &app.update_error {
        format!("failed: {error}")
    } else {
        "current".into()
    }
}

fn preview_message_line(app: &App, message: crate::process::PreviewMessage) -> Line<'static> {
    if message.text.is_empty() {
        return Line::from("");
    }
    let style = match message.role {
        crate::process::PreviewRole::User => Style::default()
            .fg(app.theme.text)
            .bg(preview_user_bg(app.theme))
            .add_modifier(Modifier::BOLD),
        crate::process::PreviewRole::Assistant => Style::default().fg(app.theme.text),
    };
    let prefix = match message.role {
        crate::process::PreviewRole::User => "  ",
        crate::process::PreviewRole::Assistant => "",
    };
    let suffix = match message.role {
        crate::process::PreviewRole::User => "  ",
        crate::process::PreviewRole::Assistant => "",
    };
    Line::from(Span::styled(
        format!("{prefix}{}{suffix}", message.text),
        style,
    ))
}

fn preview_user_bg(theme: widgets::Theme) -> Color {
    match theme.bg {
        Color::Rgb(r, g, b) if u16::from(r) + u16::from(g) + u16::from(b) > 384 => {
            Color::Rgb(226, 232, 240)
        }
        _ => Color::Rgb(45, 55, 72),
    }
}

fn imported_shared_source(profile: &ProfileInfo) -> Option<std::path::PathBuf> {
    let source = std::fs::read_link(profile.path.join("auth.json")).ok()?;
    source
        .starts_with(crate::shared_account::SHARED_ACCOUNTS_ROOT)
        .then_some(source)
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

fn used_days(value: Option<chrono::DateTime<chrono::Local>>) -> String {
    let Some(value) = value else {
        return "-".into();
    };
    let days = (chrono::Local::now() - value).num_days().max(0);
    if days == 1 {
        "1 day".into()
    } else {
        format!("{days} days")
    }
}

fn reset_time(value: Option<chrono::DateTime<chrono::Local>>) -> String {
    value
        .map(|value| value.format("%m-%d %H:%M").to_string())
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
