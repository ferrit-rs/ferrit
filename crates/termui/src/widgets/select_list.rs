use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::list::ListState;

/// Selectable rows with a full-width highlight on the active row.
///
/// Selection movement stays owned by the parent app.
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
        let lines = self.lines(area, 0, self.selected);
        frame.render_widget(Paragraph::new(lines), area);
    }

    /// Render a viewport-aware list and keep selection state synchronized.
    pub fn render_stateful(self, frame: &mut Frame<'_>, area: Rect, state: &mut ListState) {
        state.set_len(self.items.len());
        state.set_viewport_len(usize::from(area.height));
        let selected = state.selected().unwrap_or(self.selected);
        let lines = self.lines(area, state.offset(), selected);
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn lines(&self, area: Rect, offset: usize, selected: usize) -> Vec<Line<'static>> {
        self.items
            .iter()
            .cloned()
            .enumerate()
            .skip(offset)
            .take(usize::from(area.height))
            .map(|(index, mut line)| {
                if index == selected {
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
            .collect()
    }
}
