//! Key hint and inline confirmation bar.

use crate::theme::palette::Palette;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// Ferrit's shared key hints and inline confirmation prompt renderer.
pub struct KeyBar(Line<'static>);

impl KeyBar {
    pub fn hints(raw: &str, palette: &Palette) -> Self {
        Self(crate::theme::palette::keybar_line(raw, palette))
    }

    pub fn confirm(message: &str, palette: &Palette) -> Self {
        Self(crate::theme::palette::confirm_line(message, palette))
    }

    pub fn line(self) -> Line<'static> {
        self.0
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line()), area);
    }
}
