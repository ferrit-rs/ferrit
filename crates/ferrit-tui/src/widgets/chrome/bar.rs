//! Key hint and inline confirmation bar.

use ferrit_theme::palette::Palette;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Ferrit's shared key hints and inline confirmation prompt renderer.
pub struct KeyBar(Line<'static>);

impl KeyBar {
    pub fn hints(raw: &str, palette: &Palette) -> Self {
        let mut spans = Vec::new();
        for (index, segment) in raw.split(" | ").enumerate() {
            if index > 0 {
                spans.push(Span::styled(" | ", foreground(palette.idle)));
            }
            match segment.split_once(": ") {
                Some((label, key)) => {
                    spans.push(Span::styled(format!("{label}: "), foreground(palette.idle)));
                    spans.push(Span::styled(key.to_owned(), foreground(palette.key)));
                },
                None => spans.push(Span::styled(segment.to_owned(), foreground(palette.idle))),
            }
        }
        Self(Line::from(spans))
    }

    pub fn confirm(message: &str, palette: &Palette) -> Self {
        Self(Line::from(vec![
            Span::styled(
                message.to_owned(),
                foreground(palette.warn).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("Enter/y", foreground(palette.key)),
            Span::raw(" yes    "),
            Span::styled("n", foreground(palette.key)),
            Span::raw(" / "),
            Span::styled("Esc", foreground(palette.key)),
            Span::raw(" cancel"),
        ]))
    }

    pub fn line(self) -> Line<'static> {
        self.0
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line()), area);
    }
}

fn foreground(color: ratatui::style::Color) -> Style {
    Style::new().fg(color)
}
