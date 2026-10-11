use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Radio option renderer. The parent app owns group selection.
#[must_use]
pub struct Radio<'a> {
    label: &'a str,
    selected: bool,
    focused: bool,
    disabled: bool,
    style: Style,
    selected_style: Style,
}

impl<'a> Radio<'a> {
    pub fn new(label: &'a str, selected: bool) -> Self {
        Self {
            label,
            selected,
            focused: false,
            disabled: false,
            style: Style::default(),
            selected_style: Style::default().fg(Color::Rgb(96, 165, 250)),
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

    pub fn selected_style(mut self, style: Style) -> Self {
        self.selected_style = style;
        self
    }

    pub fn line(&self) -> Line<'static> {
        let marker = if self.selected { "(●)" } else { "( )" };
        let mut style = self.style;
        if self.selected {
            style = self.selected_style.patch(style);
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
    use crate::widgets::radio::Radio;

    #[test]
    fn unselected_radio_has_empty_marker() {
        assert_eq!(Radio::new("Main", false).line().to_string(), "( ) Main");
    }

    #[test]
    fn selected_radio_has_filled_marker() {
        assert_eq!(Radio::new("Main", true).line().to_string(), "(●) Main");
    }
}
