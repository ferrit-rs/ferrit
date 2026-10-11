use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Compact status label with shadcn-like visual variants.
#[must_use]
pub struct Badge<'a> {
    label: &'a str,
    style: Style,
    variant: BadgeVariant,
    shape: BadgeShape,
}

/// Badge color treatment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    #[default]
    Default,
    Secondary,
    Destructive,
    Success,
    Warning,
    Outline,
}

/// Badge end shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BadgeShape {
    #[default]
    Rectangular,
    Rounded,
}

impl BadgeVariant {
    fn style(self) -> Style {
        let white = Color::Rgb(250, 250, 250);
        match self {
            Self::Default => Style::default().fg(white).bg(Color::Rgb(37, 99, 235)),
            Self::Secondary => Style::default().fg(white).bg(Color::Rgb(63, 63, 70)),
            Self::Destructive => Style::default().fg(white).bg(Color::Rgb(220, 38, 38)),
            Self::Success => Style::default().fg(white).bg(Color::Rgb(22, 163, 74)),
            Self::Warning => Style::default()
                .fg(Color::Rgb(24, 24, 27))
                .bg(Color::Rgb(250, 204, 21)),
            Self::Outline => Style::default()
                .fg(Color::Rgb(228, 228, 231))
                .add_modifier(Modifier::BOLD),
        }
    }
}

impl<'a> Badge<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            style: Style::default(),
            variant: BadgeVariant::Default,
            shape: BadgeShape::default(),
        }
    }

    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn shape(mut self, shape: BadgeShape) -> Self {
        self.shape = shape;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn line(&self) -> Line<'static> {
        let style = self.variant.style().patch(self.style);
        let content = format!(" {} ", self.label);
        match self.shape {
            BadgeShape::Rectangular => Line::from(Span::styled(format!("[{content}]"), style)),
            BadgeShape::Rounded => Line::from(vec![
                Span::styled("(", style),
                Span::styled(content, style),
                Span::styled(")", style),
            ]),
        }
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line()), area);
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::badge::{Badge, BadgeShape, BadgeVariant};

    #[test]
    fn default_badge_has_brackets() {
        assert_eq!(Badge::new("Ready").line().to_string(), "[ Ready ]");
    }

    #[test]
    fn rounded_badge_has_parentheses() {
        let badge = Badge::new("Ready")
            .shape(BadgeShape::Rounded)
            .variant(BadgeVariant::Success);
        assert_eq!(badge.line().to_string(), "( Ready )");
    }
}
