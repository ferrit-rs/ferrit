//! Small shared palette and line helpers for reusable UI primitives.

use ratatui::style::{Color, Modifier, Style};

use super::palette::Palette;
use ratatui::text::{Line, Span};

fn fg(color: Color) -> Style {
    Style::new().fg(color)
}

pub fn confirm_line(message: &str, p: &Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(message.to_owned(), fg(p.warn).add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled("y", fg(p.key)),
        Span::raw(" yes    "),
        Span::styled("n", fg(p.key)),
        Span::raw(" / "),
        Span::styled("Esc", fg(p.key)),
        Span::raw(" cancel"),
    ])
}

pub fn keybar_line(raw: &str, p: &Palette) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, segment) in raw.split(" | ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", fg(p.idle)));
        }
        match segment.split_once(": ") {
            Some((label, key)) => {
                spans.push(Span::styled(format!("{label}: "), fg(p.idle)));
                spans.push(Span::styled(key.to_owned(), fg(p.key)));
            },
            None => spans.push(Span::styled(segment.to_owned(), fg(p.idle))),
        }
    }
    Line::from(spans)
}
