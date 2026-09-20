use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

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
