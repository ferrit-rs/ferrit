//! Reusable labeled horizontal divider for terminal sections.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const ZERO_WIDTH: usize = 0;
const LABEL_PADDING_CELLS: usize = 2;
const CENTER_SPLIT_DIVISOR: usize = 2;

/// Horizontal divider, optionally labeled. Can render alone or join a
/// scrollable `Paragraph` as a line through [`Separator::line`].
#[derive(Debug, Clone)]
pub struct Separator {
    label: String,
    style: Style,
    glyph: char,
}

impl Separator {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            style: Style::new().fg(Color::DarkGray),
            glyph: '─',
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn glyph(mut self, glyph: char) -> Self {
        self.glyph = glyph;
        self
    }

    /// Build divider line sized to terminal cells, suitable for scrollable text.
    pub fn line(&self, width: u16) -> Line<'static> {
        let width = usize::from(width);
        if width == ZERO_WIDTH {
            return Line::default();
        }
        let label = self.label.trim();
        if label.is_empty() {
            return Line::styled(self.glyph.to_string().repeat(width), self.style);
        }

        let label_width = Line::from(label.to_owned()).width();
        if label_width + LABEL_PADDING_CELLS >= width {
            return Line::styled(label.to_owned(), self.style);
        }
        let remaining = width - label_width - LABEL_PADDING_CELLS;
        let left = remaining / CENTER_SPLIT_DIVISOR;
        let right = remaining - left;
        Line::from(vec![
            Span::styled(self.glyph.to_string().repeat(left), self.style),
            Span::styled(format!(" {label} "), self.style),
            Span::styled(self.glyph.to_string().repeat(right), self.style),
        ])
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line(area.width)), area);
    }
}
