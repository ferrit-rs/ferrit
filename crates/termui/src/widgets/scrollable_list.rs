use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{List, ListState};

use crate::widgets::scrollbar::ScrollBar;

/// Stateful list with optional detached scrolling and a vertical scrollbar.
#[must_use]
pub struct ScrollableList<'a> {
    items: Vec<Line<'static>>,
    block: ratatui::widgets::Block<'a>,
    selected: Option<usize>,
    offset: usize,
    detached: bool,
    highlight_style: Style,
    scrollbar_style: Style,
}

impl<'a> ScrollableList<'a> {
    pub fn new(items: Vec<Line<'static>>, block: ratatui::widgets::Block<'a>) -> Self {
        Self {
            items,
            block,
            selected: None,
            offset: 0,
            detached: false,
            highlight_style: Style::default(),
            scrollbar_style: Style::default(),
        }
    }

    pub fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// Keep the supplied offset when selection is outside the viewport.
    pub fn detached(mut self, detached: bool) -> Self {
        self.detached = detached;
        self
    }

    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    pub fn scrollbar_style(mut self, style: Style) -> Self {
        self.scrollbar_style = style;
        self
    }

    /// Render list and return final viewport offset after selection adjustment.
    pub fn render(self, frame: &mut Frame<'_>, area: Rect) -> usize {
        let viewport_height = self.block.inner(area).height as usize;
        let row_count = self.items.len();
        let offset = if self.detached {
            self.offset.min(row_count.saturating_sub(viewport_height))
        } else {
            self.offset
        };
        let mut state = ListState::default().with_offset(offset);
        if let Some(selected) = self.selected.filter(|_| row_count > 0) {
            let selected = selected.min(row_count - 1);
            if !self.detached || (offset..offset + viewport_height).contains(&selected) {
                state.select(Some(selected));
            }
        }

        let list = List::new(self.items)
            .block(self.block)
            .highlight_style(self.highlight_style);
        frame.render_stateful_widget(list, area, &mut state);
        ScrollBar::new(row_count, viewport_height, state.offset())
            .style(self.scrollbar_style)
            .inset(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            })
            .render(frame, area);

        state.offset()
    }
}
