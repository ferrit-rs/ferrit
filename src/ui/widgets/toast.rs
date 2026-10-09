use std::error::Error;
use std::time::Duration;

use ratatui::Frame;
use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Alignment, Constraint, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use crate::theme::palette::Palette;
use crate::ui::widgets::tui_overlay::anchor::Anchor;
use crate::ui::widgets::tui_overlay::backdrop::Backdrop;
use crate::ui::widgets::tui_overlay::overlay::Overlay;
use crate::ui::widgets::tui_overlay::slide::Slide;
use crate::ui::widgets::tui_overlay::state::OverlayState;

const ANIMATION_TIME: Duration = Duration::from_millis(160);

/// How long an error stays on screen once it has slid in, before it closes by
/// itself. The message also stays in the Status pane, so nothing is lost.
const LINGER: Duration = Duration::from_secs(8);

/// Cells of text per row inside the toast (its width less the border).
const TEXT_WIDTH: usize = 52;
/// The most text rows the toast grows to.
const MAX_TEXT_ROWS: usize = 8;

/// Error notification in the terminal's bottom-right. It closes on its `x`,
/// on `Esc`, or by itself after `LINGER`.
pub struct Toast {
    error: Box<dyn Error + Send + Sync>,
    state: OverlayState,
    closing: bool,
    /// Time spent fully open, counted towards `LINGER`.
    shown: Duration,
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
            shown: Duration::ZERO,
        }
    }

    /// Start closing it (the slide-out plays, then `is_closed`). A no-op when it
    /// is already closing.
    pub fn dismiss(&mut self) {
        if !self.closing {
            self.closing = true;
            self.state.close();
        }
    }

    /// It has been dismissed or timed out and is sliding out (or gone).
    pub fn is_closing(&self) -> bool {
        self.closing
    }

    /// Rows the message needs, so a long error is not cut at two lines.
    fn text_rows(&self) -> u16 {
        let chars = self.error.to_string().chars().count();
        let rows = chars.div_ceil(TEXT_WIDTH).clamp(2, MAX_TEXT_ROWS);
        u16::try_from(rows).unwrap_or(2)
    }

    pub fn is_animating(&self) -> bool {
        self.state.is_animating()
    }

    pub fn is_closed(&self) -> bool {
        self.state.is_closed()
    }

    /// Advance the slide animation, and the time it has been fully open: after
    /// `LINGER` it starts closing by itself.
    pub fn tick(&mut self, elapsed: Duration) {
        self.state.tick(elapsed);
        if !self.closing && !self.state.is_animating() {
            self.shown += elapsed;
            if self.shown >= LINGER {
                self.dismiss();
            }
        }
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
        if close.contains(pos) {
            self.dismiss();
        }
        true
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect, palette: &Palette) {
        let border = Style::new().fg(palette.del).add_modifier(Modifier::BOLD);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(border)
            .title(Line::styled(" Error ", border));
        let overlay = Overlay::new()
            .anchor(Anchor::BottomRight)
            .slide(Slide::Bottom)
            .backdrop(Backdrop::new(Color::Black).fg(Color::DarkGray))
            .width(Constraint::Length(56))
            .height(Constraint::Length(self.text_rows() + 2))
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
