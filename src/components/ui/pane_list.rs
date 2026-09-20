use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListState};

use super::scroll_bar::ScrollBar;

#[must_use]
pub struct PaneList<'a> {
    items: Vec<Line<'static>>,
    block: Block<'a>,
    selected: Option<usize>,
    offset: usize,
    highlight_style: Style,
    scrollbar_style: Style,
}

impl<'a> PaneList<'a> {
    pub fn new(items: Vec<Line<'static>>, block: Block<'a>) -> Self {
        Self {
            items,
            block,
            selected: None,
            offset: 0,
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

    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    pub fn scrollbar_style(mut self, style: Style) -> Self {
        self.scrollbar_style = style;
        self
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) -> usize {
        let viewport_height = self.block.inner(area).height as usize;
        let row_count = self.items.len();
        let mut state = ListState::default().with_offset(self.offset);
        if let Some(selected) = self.selected.filter(|_| row_count > 0) {
            state.select(Some(selected.min(row_count - 1)));
        }

        let list = List::new(self.items)
            .block(self.block)
            .highlight_style(self.highlight_style);
        frame.render_stateful_widget(list, area, &mut state);
        ScrollBar::new(row_count, viewport_height, state.offset())
            .style(self.scrollbar_style)
            .render(
                frame,
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
            );

        state.offset()
    }
}
