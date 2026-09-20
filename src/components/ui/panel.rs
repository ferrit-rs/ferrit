use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType};

/// Ferrit's shared rounded panel shell, ready to attach to any Ratatui widget.
#[derive(Default)]
#[must_use]
pub struct Panel<'a> {
    title: Option<Line<'a>>,
    bottom_title: Option<Line<'a>>,
    border_style: Style,
}

impl<'a> Panel<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title<T: Into<Line<'a>>>(mut self, title: T) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn bottom_title<T: Into<Line<'a>>>(mut self, title: T) -> Self {
        self.bottom_title = Some(title.into());
        self
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    pub fn block(self) -> Block<'a> {
        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.border_style);
        if let Some(title) = self.title {
            block = block.title(title);
        }
        if let Some(title) = self.bottom_title {
            block = block.title_bottom(title);
        }
        block
    }
}
