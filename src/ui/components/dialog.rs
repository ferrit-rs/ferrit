use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Clear};

/// Rendered regions inside a dialog shell.
#[derive(Debug, Clone, Copy)]
pub struct DialogAreas {
    pub outer: Rect,
    pub body: Rect,
    /// Empty rectangle when this dialog has no footer region.
    pub footer: Rect,
}

/// Centered, bordered overlay shell. Render body and footer widgets into the
/// returned areas to compose dialogs without coupling them to app state.
#[must_use]
pub struct Dialog<'a> {
    title: Line<'a>,
    width: u16,
    height: u16,
    footer_rows: Option<u16>,
    border_style: Style,
}

impl<'a> Dialog<'a> {
    pub fn new<T: Into<Line<'a>>>(title: T) -> Self {
        Self {
            title: title.into(),
            width: u16::MAX,
            height: u16::MAX,
            footer_rows: None,
            border_style: Style::default(),
        }
    }

    pub fn size(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn footer_rows(mut self, rows: u16) -> Self {
        self.footer_rows = Some(rows);
        self
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Clear the overlay area, draw its shell, and return child regions.
    pub fn render(self, frame: &mut Frame<'_>, area: Rect) -> DialogAreas {
        let width = self.width.min(area.width);
        let height = self.height.min(area.height);
        let outer = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y: area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .title(self.title)
            .border_style(self.border_style);
        let inner = block.inner(outer);

        frame.render_widget(Clear, outer);
        frame.render_widget(block, outer);

        let (body, footer) = match self.footer_rows {
            Some(rows) => Layout::vertical([Constraint::Min(1), Constraint::Length(rows)])
                .areas(inner)
                .into(),
            None => (inner, Rect::default()),
        };
        DialogAreas {
            outer,
            body,
            footer,
        }
    }
}
