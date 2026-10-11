use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Compact on/off switch. The parent app owns the boolean state and events.
#[must_use]
pub struct Toggle<'a> {
    label: &'a str,
    enabled: bool,
    focused: bool,
    disabled: bool,
    style: Style,
    enabled_style: Style,
}

impl<'a> Toggle<'a> {
    pub fn new(label: &'a str, enabled: bool) -> Self {
        Self {
            label,
            enabled,
            focused: false,
            disabled: false,
            style: Style::default(),
            enabled_style: Style::default().fg(Color::Rgb(74, 222, 128)),
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

    pub fn enabled_style(mut self, style: Style) -> Self {
        self.enabled_style = style;
        self
    }

    pub fn line(&self) -> Line<'static> {
        let state = if self.enabled { "[ON ]" } else { "[OFF]" };
        let mut style = self.style;
        if self.enabled {
            style = self.enabled_style.patch(style);
        }
        if self.focused {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.disabled {
            style = style.add_modifier(Modifier::DIM);
        }
        Line::from(vec![
            Span::styled(state, style),
            Span::styled(format!(" {}", self.label), style),
        ])
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line()), area);
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::toggle::Toggle;

    #[test]
    fn disabled_toggle_has_off_state() {
        assert_eq!(
            Toggle::new("Watch", false).line().to_string(),
            "[OFF] Watch"
        );
    }

    #[test]
    fn enabled_toggle_has_on_state() {
        assert_eq!(Toggle::new("Watch", true).line().to_string(), "[ON ] Watch");
    }
}
