//! Small shared palette and line helpers for reusable UI primitives.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub const FOCUS: Color = Color::Green;
pub const DEL: Color = Color::Red;
pub const WARN: Color = Color::Yellow;
pub const KEY: Color = Color::Yellow;
pub const IDLE: Color = Color::Gray;

fn fg(color: Color) -> Style {
    Style::new().fg(color)
}

pub fn confirm_line(message: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(message.to_owned(), fg(WARN).add_modifier(Modifier::BOLD)),
        Span::raw("   "),
        Span::styled("y", fg(KEY)),
        Span::raw(" yes    "),
        Span::styled("n", fg(KEY)),
        Span::raw(" / "),
        Span::styled("Esc", fg(KEY)),
        Span::raw(" cancel"),
    ])
}

pub fn keybar_line(raw: &'static str) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, segment) in raw.split(" | ").enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", fg(IDLE)));
        }
        match segment.split_once(": ") {
            Some((label, key)) => {
                spans.push(Span::styled(format!("{label}: "), fg(IDLE)));
                spans.push(Span::styled(key.to_owned(), fg(KEY)));
            },
            None => spans.push(Span::styled(segment.to_owned(), fg(IDLE))),
        }
    }
    Line::from(spans)
}
