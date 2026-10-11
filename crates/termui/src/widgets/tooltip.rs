use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

/// Small bordered help bubble rendered in a caller-provided area.
#[must_use]
pub struct Tooltip {
    message: String,
    title: Option<String>,
    style: Style,
}

impl Tooltip {
    pub fn new<M: Into<String>>(message: M) -> Self {
        Self {
            message: message.into(),
            title: None,
            style: Style::default().fg(Color::Rgb(228, 228, 231)),
        }
    }

    pub fn title<T: Into<String>>(mut self, title: T) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.style);
        if let Some(title) = self.title {
            block = block.title(Line::from(title));
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
    use crate::widgets::tooltip::Tooltip;

    #[test]
    fn tooltip_accepts_optional_title() {
        let _ = Tooltip::new("Open the command menu").title("Shortcut");
    }
}
