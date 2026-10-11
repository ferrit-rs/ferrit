use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

/// Bordered message with a semantic visual variant.
#[must_use]
pub struct Alert {
    title: String,
    message: String,
    variant: AlertVariant,
    style: Style,
}

/// Alert color treatment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertVariant {
    #[default]
    Default,
    Info,
    Success,
    Warning,
    Destructive,
}

impl AlertVariant {
    fn style(self) -> Style {
        match self {
            Self::Default => Style::default().fg(Color::Rgb(228, 228, 231)),
            Self::Info => Style::default().fg(Color::Rgb(96, 165, 250)),
            Self::Success => Style::default().fg(Color::Rgb(74, 222, 128)),
            Self::Warning => Style::default().fg(Color::Rgb(250, 204, 21)),
            Self::Destructive => Style::default()
                .fg(Color::Rgb(248, 113, 113))
                .add_modifier(Modifier::BOLD),
        }
    }
}

impl Alert {
    pub fn new<T: Into<String>, M: Into<String>>(title: T, message: M) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            variant: AlertVariant::default(),
            style: Style::default(),
        }
    }

    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        let style = self.variant.style().patch(self.style);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style)
            .title(Line::styled(self.title, style));
        frame.render_widget(
            Paragraph::new(self.message)
                .style(style)
                .wrap(Wrap { trim: true })
                .block(block),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::alert::{Alert, AlertVariant};

    #[test]
    fn alert_accepts_semantic_variant() {
        let _ = Alert::new("Build", "Build completed").variant(AlertVariant::Success);
    }
}
