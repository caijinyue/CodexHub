use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub const BG: Color = Color::Rgb(12, 16, 24);
pub const BORDER: Color = Color::Rgb(70, 87, 115);
pub const TITLE: Color = Color::Rgb(124, 209, 255);
pub const TEXT: Color = Color::Rgb(218, 226, 241);
pub const MUTED: Color = Color::Rgb(143, 155, 179);
pub const OK: Color = Color::Rgb(62, 211, 139);
pub const WARN: Color = Color::Rgb(244, 191, 79);
pub const ERROR: Color = Color::Rgb(255, 103, 110);
pub const ACTION: Color = Color::Rgb(159, 140, 255);
pub const CACHE: Color = Color::Rgb(84, 210, 196);
pub const SYSTEM: Color = Color::Rgb(255, 153, 102);

pub fn block(title: &str) -> Block<'_> {
    Block::default()
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(TITLE).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().fg(TEXT).bg(BG))
}

pub fn header_style() -> Style {
    Style::default()
        .fg(TITLE)
        .bg(BG)
        .add_modifier(Modifier::BOLD)
}

pub fn selected_style() -> Style {
    Style::default().fg(Color::Black).bg(TITLE)
}

pub fn help_bar(groups: &[(&str, &[(&str, &str)])]) -> Paragraph<'static> {
    let mut spans = Vec::new();
    for (idx, (label, keys)) in groups.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw("  "));
        }
        let color = help_color(idx);
        spans.push(Span::styled(
            format!("{label}: "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        for (key_idx, (key, action)) in keys.iter().enumerate() {
            if key_idx > 0 {
                spans.push(Span::styled(" / ", Style::default().fg(MUTED)));
            }
            spans.push(Span::styled(
                (*key).to_string(),
                Style::default()
                    .fg(Color::Black)
                    .bg(color)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {action}"),
                Style::default().fg(TEXT),
            ));
        }
    }
    Paragraph::new(Line::from(spans))
        .block(block("Keys"))
        .style(Style::default().fg(TEXT).bg(BG))
}

pub fn help_color(index: usize) -> Color {
    match index % 5 {
        0 => TITLE,
        1 => ACTION,
        2 => OK,
        3 => CACHE,
        _ => SYSTEM,
    }
}
