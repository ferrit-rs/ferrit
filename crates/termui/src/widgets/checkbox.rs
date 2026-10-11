use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Checkbox renderer. The parent app owns the boolean state and events.
#[must_use]
pub struct Checkbox<'a> {
    label: &'a str,
    checked: bool,
    focused: bool,
    disabled: bool,
    style: Style,
    checked_style: Style,
}

impl<'a> Checkbox<'a> {
    pub fn new(label: &'a str, checked: bool) -> Self {
        Self {
            label,
            checked,
            focused: false,
            disabled: false,
            style: Style::default(),
            checked_style: Style::default().fg(Color::Rgb(96, 165, 250)),
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn checked_style(mut self, style: Style) -> Self {
        self.checked_style = style;
        self
    }

    pub fn line(&self) -> Line<'static> {
        let marker = if self.checked { "[x]" } else { "[ ]" };
        let mut style = self.style;
        if self.checked {
            style = self.checked_style.patch(style);
        }
        if self.focused {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.disabled {
            style = style.add_modifier(Modifier::DIM);
        }
        Line::from(vec![
            Span::styled(marker, style),
            Span::styled(format!(" {}", self.label), style),
        ])
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line()), area);
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::checkbox::Checkbox;

    #[test]
    fn unchecked_checkbox_has_empty_marker() {
        assert_eq!(Checkbox::new("Sync", false).line().to_string(), "[ ] Sync");
    }

    #[test]
    fn checked_checkbox_has_checked_marker() {
        assert_eq!(Checkbox::new("Sync", true).line().to_string(), "[x] Sync");
    }
}
