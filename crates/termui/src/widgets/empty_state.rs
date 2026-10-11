use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

/// Centered title and message for an empty collection or screen.
#[must_use]
pub struct EmptyState {
    title: String,
    message: String,
    style: Style,
}

impl EmptyState {
    pub fn new<T: Into<String>, M: Into<String>>(title: T, message: M) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            style: Style::default(),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        let title_style = self.style.add_modifier(Modifier::BOLD);
        let lines = vec![
            Line::from(Span::styled(self.title, title_style)),
            Line::from(self.message),
        ];
        let block = Block::bordered().border_type(BorderType::Rounded);
        frame.render_widget(
            Paragraph::new(lines)
                .alignment(Alignment::Center)
                .style(self.style)
                .block(block),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::empty_state::EmptyState;

    #[test]
    fn empty_state_builds_from_owned_text() {
        let _ = EmptyState::new("No commits", "Create a commit to see it here");
    }
}
