use std::error::Error;
use std::time::Duration;

use ratatui::Frame;
use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Alignment, Constraint, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use crate::components::tui_overlay::anchor::Anchor;
use crate::components::tui_overlay::backdrop::Backdrop;
use crate::components::tui_overlay::overlay::Overlay;
use crate::components::tui_overlay::slide::Slide;
use crate::components::tui_overlay::state::OverlayState;
use crate::components::ui::style as theme;

const ANIMATION_TIME: Duration = Duration::from_millis(160);

/// Persistent error notification, rendered in the terminal's bottom-right.
pub struct Toast {
    error: Box<dyn Error + Send + Sync>,
    state: OverlayState,
    closing: bool,
}

impl Toast {
    pub fn error<E>(error: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        let error: Box<dyn Error + Send + Sync> = Box::new(error);
        let mut state = OverlayState::new().with_duration(ANIMATION_TIME);
        state.open();
        Self {
            error,
            state,
            closing: false,
        }
    }

    pub fn is_animating(&self) -> bool {
        self.state.is_animating()
    }

    pub fn is_closed(&self) -> bool {
        self.state.is_closed()
    }

    pub fn tick(&mut self, elapsed: Duration) {
        self.state.tick(elapsed);
    }

    /// Consume clicks inside toast. Only its `x` button closes it.
    pub fn on_mouse(&mut self, event: MouseEvent) -> bool {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        let Some(rect) = self.state.overlay_rect() else {
            return false;
        };
        let pos = Position::new(event.column, event.row);
        if !rect.contains(pos) {
            return false;
        }

        let close = Rect::new(rect.right().saturating_sub(5), rect.y, 5, 1);
        if close.contains(pos) && !self.closing {
            self.closing = true;
            self.state.close();
        }
        true
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let border = Style::new().fg(theme::DEL).add_modifier(Modifier::BOLD);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(border)
            .title(Line::styled(" Error ", border));
        let overlay = Overlay::new()
            .anchor(Anchor::BottomRight)
            .slide(Slide::Bottom)
            .backdrop(Backdrop::new(Color::Black).fg(Color::DarkGray))
            .width(Constraint::Length(56))
            .height(Constraint::Length(4))
            .block(block);
        frame.render_stateful_widget(overlay, area, &mut self.state);
        if let Some(rect) = self.state.overlay_rect() {
            frame.render_widget(
                Paragraph::new(" x ")
                    .alignment(Alignment::Right)
                    .style(border),
                Rect::new(rect.right().saturating_sub(5), rect.y, 5, 1),
            );
        }
        if let Some(inner) = self.state.inner_area() {
            frame.render_widget(
                Paragraph::new(self.error.to_string()).wrap(Wrap { trim: true }),
                inner,
            );
        }
    }
}
