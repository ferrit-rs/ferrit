//! Selectable and scrollable list widgets.

use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListState, Scrollbar, ScrollbarOrientation, ScrollbarState};

/// Vertical scrollbar shared by pane lists and scrollable previews.
#[must_use]
pub struct ScrollBar {
    content_length: usize,
    viewport_length: usize,
    position: usize,
    style: Style,
}

impl ScrollBar {
    pub fn new(content_length: usize, viewport_length: usize, position: usize) -> Self {
        Self {
            content_length,
            viewport_length,
            position,
            style: Style::default(),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Draw only when content exceeds viewport. `area` is the scrollbar track.
    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        if self.content_length <= self.viewport_length {
            return;
        }

        let max_scroll = self.content_length - self.viewport_length;
        let mut state = ScrollbarState::new(max_scroll + 1)
            .position(self.position.min(max_scroll))
            .viewport_content_length(self.viewport_length);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(self.style)
                .begin_symbol(None)
                .end_symbol(None),
            area,
            &mut state,
        );
    }
}

/// Render selectable rows with a full-width style on the active row.
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
        frame.render_widget(ratatui::widgets::Paragraph::new(lines), area);
    }
}

#[must_use]
pub struct PaneList<'a> {
    items: Vec<Line<'static>>,
    block: ratatui::widgets::Block<'a>,
    selected: Option<usize>,
    offset: usize,
    detached: bool,
    highlight_style: Style,
    scrollbar_style: Style,
}

impl<'a> PaneList<'a> {
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

    /// The view was scrolled on its own: keep `offset` even when selected is
    /// off screen, and draw no highlight then.
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
