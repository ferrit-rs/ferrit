use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

/// Modal content shell. The parent chooses its screen area and open state.
#[must_use]
pub struct Dialog {
    title: String,
    message: String,
    footer: Option<String>,
    style: Style,
}

impl Dialog {
    pub fn new<T: Into<String>, M: Into<String>>(title: T, message: M) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            footer: None,
            style: Style::default().fg(Color::Rgb(228, 228, 231)),
        }
    }

    pub fn footer<F: Into<String>>(mut self, footer: F) -> Self {
        self.footer = Some(footer.into());
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        let title_style = self.style.add_modifier(Modifier::BOLD);
        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.style)
            .title(Line::styled(self.title, title_style));
        if let Some(footer) = self.footer {
            block = block.title_bottom(Line::from(Span::styled(footer, self.style)));
        }
        frame.render_widget(
            Paragraph::new(self.message)
                .style(self.style)
                .wrap(Wrap { trim: true })
                .block(block),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::dialog::Dialog;

    #[test]
    fn dialog_accepts_optional_footer() {
        let _ = Dialog::new("Confirm", "Continue?").footer("Enter confirm | Esc cancel");
    }
}
