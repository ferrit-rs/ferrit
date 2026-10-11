use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Widget};

/// Bordered form control shell with optional hint or validation error.
#[derive(Debug, Clone)]
#[must_use]
pub struct FormField<'a> {
    label: Line<'a>,
    hint: Option<Line<'a>>,
    error: Option<Line<'a>>,
    border_style: Style,
    hint_style: Style,
    error_style: Style,
}

impl<'a> FormField<'a> {
    pub fn new<T: Into<Line<'a>>>(label: T) -> Self {
        Self {
            label: label.into(),
            hint: None,
            error: None,
            border_style: Style::default(),
            hint_style: Style::default().fg(Color::DarkGray),
            error_style: Style::default().fg(Color::Red),
        }
    }

    pub fn hint<T: Into<Line<'a>>>(mut self, hint: T) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn error<T: Into<Line<'a>>>(mut self, error: T) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    pub fn hint_style(mut self, style: Style) -> Self {
        self.hint_style = style;
        self
    }

    pub fn error_style(mut self, style: Style) -> Self {
        self.error_style = style;
        self
    }

    pub fn block(&self) -> Block<'a> {
        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.border_style)
            .title(self.label.clone());
        if let Some(error) = self.error.as_ref() {
            block = block.title_bottom(error.clone().patch_style(self.error_style));
        } else if let Some(hint) = self.hint.as_ref() {
            block = block.title_bottom(hint.clone().patch_style(self.hint_style));
        }
        block
    }

    pub fn inner(&self, area: Rect) -> Rect {
        self.block().inner(area)
    }

    /// Render field chrome and any Ratatui widget inside it.
    pub fn render<W: Widget>(self, frame: &mut ratatui::Frame<'_>, area: Rect, body: W) {
        let block = self.block();
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(body, inner);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use crate::widgets::form_field::FormField;

    #[test]
    fn error_replaces_hint_in_field_chrome() {
        let field = FormField::new("Name").hint("Optional").error("Required");
        assert_eq!(field.inner(Rect::new(0, 0, 20, 4)).height, 2);
    }

    #[test]
    fn field_inner_area_respects_border() {
        let field = FormField::new("Name");
        assert_eq!(field.inner(Rect::new(0, 0, 20, 4)), Rect::new(1, 1, 18, 2));
    }
}
