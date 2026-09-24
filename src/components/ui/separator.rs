//! Reusable labeled horizontal divider for terminal sections.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const ZERO_WIDTH: usize = 0;
const LABEL_PADDING_CELLS: usize = 2;
const CENTER_SPLIT_DIVISOR: usize = 2;
const HORIZONTAL_MARGIN_MULTIPLIER: usize = 2;

/// Horizontal divider, optionally labeled. Can render alone or join a
/// scrollable `Paragraph` as a line through [`Separator::line`].
#[derive(Debug, Clone)]
pub struct Separator {
    label: String,
    style: Style,
    glyph: char,
    margin_x: u16,
    margin_y: u16,
}

impl Separator {
    pub fn new<L: Into<String>>(label: L) -> Self {
        Self {
            label: label.into(),
            style: Style::new().fg(Color::DarkGray),
            glyph: '─',
            margin_x: 0,
            margin_y: 0,
        }
    }

    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    #[must_use]
    pub fn glyph(mut self, glyph: char) -> Self {
        self.glyph = glyph;
        self
    }

    /// Set equal left and right margins, measured in terminal cells.
    #[must_use]
    pub fn margin_x(mut self, cells: u16) -> Self {
        self.margin_x = cells;
        self
    }

    /// Set equal top and bottom margins, measured in terminal rows.
    #[must_use]
    pub fn margin_y(mut self, rows: u16) -> Self {
        self.margin_y = rows;
        self
    }

    /// Build divider line sized to terminal cells, suitable for scrollable text.
    pub fn line(&self, width: u16) -> Line<'static> {
        let margin = usize::from(self.margin_x);
        let width = usize::from(width);
        let content_width = width.saturating_sub(margin * HORIZONTAL_MARGIN_MULTIPLIER);
        let mut line = self.content_line(content_width);
        if margin > ZERO_WIDTH {
            line.spans.insert(ZERO_WIDTH, Span::raw(" ".repeat(margin)));
        }
        line
    }

    fn content_line(&self, width: usize) -> Line<'static> {
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

    /// Build lines including vertical margins for scrollable `Paragraph`s.
    pub fn lines(&self, width: u16) -> Vec<Line<'static>> {
        let vertical_margin = usize::from(self.margin_y);
        let mut lines = vec![Line::default(); vertical_margin];
        lines.push(self.line(width));
        lines.extend(vec![Line::default(); vertical_margin]);
        lines
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.lines(area.width)), area);
    }
}
