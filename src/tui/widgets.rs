use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub border: Color,
    pub title: Color,
    pub text: Color,
    pub muted: Color,
    pub ok: Color,
    pub warn: Color,
    pub error: Color,
    pub action: Color,
    pub cache: Color,
    pub system: Color,
    pub selected_fg: Color,
    pub key_fg: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePreference {
    Auto,
    Light,
    Dark,
}

impl ThemePreference {
    pub fn cycle(self) -> Self {
        match self {
            Self::Auto => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::Auto,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

impl Theme {
    pub fn from_preference(preference: ThemePreference) -> Self {
        match preference {
            ThemePreference::Auto if detect_light_mode() => Self::light(),
            ThemePreference::Auto => Self::dark(),
            ThemePreference::Light => Self::light(),
            ThemePreference::Dark => Self::dark(),
        }
    }

    pub fn detected_preference() -> ThemePreference {
        if detect_light_mode() {
            ThemePreference::Light
        } else {
            ThemePreference::Dark
        }
    }

    pub fn dark() -> Self {
        Self {
            bg: Color::Rgb(12, 16, 24),
            border: Color::Rgb(70, 87, 115),
            title: Color::Rgb(124, 209, 255),
            text: Color::Rgb(218, 226, 241),
            muted: Color::Rgb(143, 155, 179),
            ok: Color::Rgb(62, 211, 139),
            warn: Color::Rgb(244, 191, 79),
            error: Color::Rgb(255, 103, 110),
            action: Color::Rgb(159, 140, 255),
            cache: Color::Rgb(84, 210, 196),
            system: Color::Rgb(255, 153, 102),
            selected_fg: Color::Black,
            key_fg: Color::Black,
        }
    }

    pub fn light() -> Self {
        Self {
            bg: Color::Rgb(248, 250, 252),
            border: Color::Rgb(148, 163, 184),
            title: Color::Rgb(3, 105, 161),
            text: Color::Rgb(15, 23, 42),
            muted: Color::Rgb(100, 116, 139),
            ok: Color::Rgb(22, 101, 52),
            warn: Color::Rgb(180, 83, 9),
            error: Color::Rgb(190, 18, 60),
            action: Color::Rgb(109, 40, 217),
            cache: Color::Rgb(15, 118, 110),
            system: Color::Rgb(194, 65, 12),
            selected_fg: Color::White,
            key_fg: Color::White,
        }
    }
}

fn detect_light_mode() -> bool {
    for name in [
        "CODEXHUB_THEME",
        "TERMINAL_THEME",
        "COLORTERM_THEME",
        "THEME",
        "BACKGROUND",
    ] {
        let Ok(value) = std::env::var(name) else {
            continue;
        };
        let value = value.to_ascii_lowercase();
        if value.contains("light") {
            return true;
        }
        if value.contains("dark") {
            return false;
        }
    }
    colorfgbg_is_light()
}

fn colorfgbg_is_light() -> bool {
    let Ok(value) = std::env::var("COLORFGBG") else {
        return false;
    };
    let Some(bg) = value
        .split([';', ':'])
        .filter_map(|part| part.parse::<u8>().ok())
        .next_back()
    else {
        return false;
    };
    matches!(bg, 7 | 15 | 10 | 11 | 14)
}

pub fn block<'a>(title: &'a str, theme: Theme) -> Block<'a> {
    Block::default()
        .title(Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().fg(theme.text).bg(theme.bg))
}

pub fn header_style(theme: Theme) -> Style {
    Style::default()
        .fg(theme.title)
        .bg(theme.bg)
        .add_modifier(Modifier::BOLD)
}

pub fn selected_style(theme: Theme) -> Style {
    Style::default().fg(theme.selected_fg).bg(theme.title)
}

pub fn help_bar(groups: &[(&str, &[(&str, &str)])], theme: Theme) -> Paragraph<'static> {
    let mut spans = Vec::new();
    for (idx, (label, keys)) in groups.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw("  "));
        }
        let color = help_color(idx, theme);
        spans.push(Span::styled(
            format!("{label}: "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        for (key_idx, (key, action)) in keys.iter().enumerate() {
            if key_idx > 0 {
                spans.push(Span::styled(" / ", Style::default().fg(theme.muted)));
            }
            spans.push(Span::styled(
                (*key).to_string(),
                Style::default()
                    .fg(theme.key_fg)
                    .bg(color)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {action}"),
                Style::default().fg(theme.text),
            ));
        }
    }
    Paragraph::new(Line::from(spans))
        .block(block("Keys", theme))
        .style(Style::default().fg(theme.text).bg(theme.bg))
}

pub fn help_color(index: usize, theme: Theme) -> Color {
    match index % 5 {
        0 => theme.title,
        1 => theme.action,
        2 => theme.ok,
        3 => theme.cache,
        _ => theme.system,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_light_colorfgbg_background() {
        let _guard = crate::test_support::env_lock();
        std::env::set_var("COLORFGBG", "0;15");
        std::env::remove_var("CODEXHUB_THEME");

        assert!(colorfgbg_is_light());

        std::env::remove_var("COLORFGBG");
    }

    #[test]
    fn light_and_dark_have_different_backgrounds() {
        assert_ne!(Theme::light().bg, Theme::dark().bg);
    }

    #[test]
    fn cycles_theme_preference() {
        assert_eq!(ThemePreference::Auto.cycle(), ThemePreference::Light);
        assert_eq!(ThemePreference::Light.cycle(), ThemePreference::Dark);
        assert_eq!(ThemePreference::Dark.cycle(), ThemePreference::Auto);
    }
}
