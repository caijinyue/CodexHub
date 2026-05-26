use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, Borders},
};

pub fn block(title: &str) -> Block<'_> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}

pub fn header_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

pub fn selected_style() -> Style {
    Style::default().fg(Color::Black).bg(Color::Cyan)
}
