use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType};

use crate::components::tui_overlay::{Anchor, Backdrop, Overlay, OverlayState, Slide};
use crate::theme;

/// Right-side drawer shell. Returns its inner area for caller-owned content.
#[must_use]
pub struct Drawer<'state, 'title> {
    state: &'state mut OverlayState,
    title: Line<'title>,
    width: Constraint,
    border_style: Style,
}

impl<'state, 'title> Drawer<'state, 'title> {
    pub fn new<T: Into<Line<'title>>>(state: &'state mut OverlayState, title: T) -> Self {
        Self {
            state,
            title: title.into(),
            width: Constraint::Percentage(50),
            border_style: Style::new().fg(theme::FOCUS),
        }
    }

    pub fn width(mut self, width: Constraint) -> Self {
        self.width = width;
        self
    }

    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Draw backdrop and frame, then return the area for drawer body widgets.
    pub fn render(self, frame: &mut Frame<'_>, area: Rect) -> Option<Rect> {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.border_style)
            .title(self.title);
        let overlay = Overlay::new()
            .anchor(Anchor::Right)
            .slide(Slide::Right)
            .width(self.width)
            .height(Constraint::Percentage(100))
            .backdrop(Backdrop::new(Color::Black).fg(Color::DarkGray))
            .block(block);
        frame.render_stateful_widget(overlay, area, self.state);
        self.state.inner_area()
    }
}
