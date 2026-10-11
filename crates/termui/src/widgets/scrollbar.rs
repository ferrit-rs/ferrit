use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

/// Vertical scrollbar for any content with a finite viewport.
#[derive(Clone, Copy, Debug, Default)]
#[must_use]
pub struct ScrollBar {
    content_length: usize,
    viewport_length: usize,
    position: usize,
    style: Style,
    inset: Margin,
}

impl ScrollBar {
    pub fn new(content_length: usize, viewport_length: usize, position: usize) -> Self {
        Self {
            content_length,
            viewport_length,
            position,
            style: Style::default(),
            inset: Margin::default(),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn inset(mut self, inset: Margin) -> Self {
        self.inset = inset;
        self
    }

    /// Draw only when content exceeds viewport.
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
            area.inner(self.inset),
            &mut state,
        );
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Margin;

    use crate::widgets::scrollbar::ScrollBar;

    #[test]
    fn scrollbar_accepts_insets() {
        let _ = ScrollBar::new(100, 20, 4).inset(Margin {
            vertical: 1,
            horizontal: 0,
        });
    }

    #[test]
    fn short_content_builds_without_scroll() {
        let _ = ScrollBar::new(3, 4, 0);
    }
}
