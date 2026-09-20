use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Render selectable rows with a full-width style on the active row.
/// Selection movement remains owned by the parent app.
#[must_use]
pub struct SelectList<'a> {
    items: &'a [Line<'static>],
    selected: usize,
    selection_style: Style,
}

impl<'a> SelectList<'a> {
    pub fn new(items: &'a [Line<'static>], selected: usize) -> Self {
        Self {
            items,
            selected,
            selection_style: Style::default(),
        }
    }

    pub fn selection_style(mut self, style: Style) -> Self {
        self.selection_style = style;
        self
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        let lines = self
            .items
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, mut line)| {
                if index == self.selected {
                    let padding = (area.width as usize).saturating_sub(line.width());
                    if padding > 0 {
                        line.spans.push(Span::raw(" ".repeat(padding)));
                    }
                    for span in &mut line.spans {
                        span.style = self.selection_style;
                    }
                }
                line
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), area);
    }
}
