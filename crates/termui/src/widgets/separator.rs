use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const LABEL_PADDING_CELLS: usize = 2;
const CENTER_SPLIT_DIVISOR: usize = 2;
const HORIZONTAL_MARGIN_MULTIPLIER: usize = 2;

/// Horizontal divider, optionally labeled.
#[derive(Debug, Clone)]
#[must_use]
pub struct Separator {
    label: String,
    style: Style,
    glyph: char,
    margin_x: u16,
}

impl Separator {
    pub fn new<L: Into<String>>(label: L) -> Self {
        Self {
            label: label.into(),
            style: Style::new().fg(Color::DarkGray),
            glyph: '─',
            margin_x: 0,
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

    pub fn margin_x(mut self, cells: u16) -> Self {
        self.margin_x = cells;
        self
    }

    pub fn line(&self, width: u16) -> Line<'static> {
        let margin = usize::from(self.margin_x);
        let content_width =
            usize::from(width).saturating_sub(margin * HORIZONTAL_MARGIN_MULTIPLIER);
        let mut line = self.content_line(content_width);
        if margin > 0 {
            line.spans.insert(0, Span::raw(" ".repeat(margin)));
        }
        line
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line(area.width)), area);
    }

    fn content_line(&self, width: usize) -> Line<'static> {
        if width == 0 {
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
}

#[cfg(test)]
mod tests {
    use crate::widgets::separator::Separator;

    #[test]
    fn labeled_separator_centers_label() {
        assert_eq!(
            Separator::new("Logs").line(14).to_string(),
            "──── Logs ────"
        );
    }

    #[test]
    fn empty_separator_fills_width() {
        assert_eq!(Separator::new("").line(4).to_string(), "────");
    }
}
