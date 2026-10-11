use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Keyboard shortcut rendered as a compact keycap.
#[must_use]
pub struct Kbd<'a> {
    key: &'a str,
    style: Style,
    key_style: Style,
}

impl<'a> Kbd<'a> {
    pub fn new(key: &'a str) -> Self {
        Self {
            key,
            style: Style::default(),
            key_style: Style::default().add_modifier(Modifier::BOLD),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn key_style(mut self, style: Style) -> Self {
        self.key_style = style;
        self
    }

    pub fn line(&self) -> Line<'static> {
        let style = self.style.patch(self.key_style);
        Line::from(vec![
            Span::styled("[", style),
            Span::styled(format!(" {} ", self.key), style),
            Span::styled("]", style),
        ])
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line()), area);
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::kbd::Kbd;

    #[test]
    fn keycap_wraps_key() {
        assert_eq!(Kbd::new("Ctrl+K").line().to_string(), "[ Ctrl+K ]");
    }
}
