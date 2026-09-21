use ratatui_core::buffer::Buffer;
use ratatui_core::layout::{Constraint, Rect};
use ratatui_core::style::Color;
use ratatui_core::widgets::{StatefulWidget, Widget};
use ratatui_widgets::block::Block;

use super::backdrop::Backdrop;
use super::layout::resolve_rect;
use super::state::{OverlayState, Phase};
use super::{anchor::Anchor, slide::Slide};

/// Composable overlay widget for Ratatui.
///
/// Configured via builder methods, rendered via [`StatefulWidget`] with [`OverlayState`].
/// The overlay does not own body content — it draws chrome and exposes
/// [`OverlayState::inner_area`] for the caller to render into.
///
/// # Defaults
///
/// | Property | Default |
/// |----------|---------|
/// | `anchor` | [`Anchor::Center`] |
/// | `width` | `Percentage(100)` |
/// | `height` | `Percentage(100)` |
/// | `offset` | `(0, 0)` |
/// | `slide` | `None` (instant appear) |
/// | `backdrop` | `None` (no dimming) |
/// | `block` | `None` (no chrome) |
/// | `bg` | `None` (transparent — no fill) |
#[derive(Debug, Clone)]
pub struct Overlay<'a> {
    anchor: Anchor,
    width: Constraint,
    height: Constraint,
    offset: (i16, i16),
    slide: Option<Slide>,
    backdrop: Option<Backdrop>,
    block: Option<Block<'a>>,
    bg: Option<Color>,
}

impl<'a> Overlay<'a> {
    pub fn new() -> Self {
        Self {
            anchor: Anchor::Center,
            width: Constraint::Percentage(100),
            height: Constraint::Percentage(100),
            offset: (0, 0),
            slide: None,
            backdrop: None,
            block: None,
            bg: None,
        }
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn width(mut self, width: Constraint) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Constraint) -> Self {
        self.height = height;
        self
    }

    pub fn offset(mut self, x: i16, y: i16) -> Self {
        self.offset = (x, y);
        self
    }

    pub fn slide(mut self, slide: Slide) -> Self {
        self.slide = Some(slide);
        self
    }

    pub fn backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = Some(backdrop);
        self
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }
}

impl Default for Overlay<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl StatefulWidget for Overlay<'_> {
    type State = OverlayState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut OverlayState) {
        if state.phase == Phase::Closed {
            state.overlay_rect = None;
            state.inner_area = None;
            return;
        }

        let area = area.intersection(*buf.area());
        let full_rect = resolve_rect(area, self.width, self.height, self.anchor, self.offset);
        let overlay_rect = clip_by_slide(full_rect, self.slide, state.visibility());

        if overlay_rect.width == 0 || overlay_rect.height == 0 {
            state.overlay_rect = None;
            state.inner_area = None;
            return;
        }

        if let Some(ref backdrop) = self.backdrop {
            backdrop.apply(buf, area, overlay_rect);
        }

        clear_region(buf, overlay_rect);

        if let Some(color) = self.bg {
            fill_bg(buf, overlay_rect, color);
        }

        let inner = match self.block {
            Some(block) => {
                let inner = block.inner(overlay_rect);

                // Skip block chrome when the rect is too small for borders
                if inner.width > 0 && inner.height > 0 {
                    block.render(overlay_rect, buf);
                }

                inner
            },
            None => overlay_rect,
        };

        state.overlay_rect = Some(overlay_rect);
        state.inner_area = Some(inner);
    }
}

fn clip_by_slide(rect: Rect, slide: Option<Slide>, visibility: f32) -> Rect {
    let slide = match slide {
        Some(s) if visibility < 1.0 => s,
        _ => return rect,
    };

    match slide {
        Slide::Right => {
            let w = (visibility * rect.width as f32).round() as u16;
            Rect::new(rect.right().saturating_sub(w), rect.y, w, rect.height)
        },

        Slide::Left => {
            let w = (visibility * rect.width as f32).round() as u16;
            Rect::new(rect.x, rect.y, w, rect.height)
        },

        Slide::Bottom => {
            let h = (visibility * rect.height as f32).round() as u16;
            Rect::new(rect.x, rect.bottom().saturating_sub(h), rect.width, h)
        },

        Slide::Top => {
            let h = (visibility * rect.height as f32).round() as u16;
            Rect::new(rect.x, rect.y, rect.width, h)
        },
    }
}

fn clear_region(buf: &mut Buffer, area: Rect) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].reset();
        }
    }
}

fn fill_bg(buf: &mut Buffer, area: Rect, color: Color) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].bg = color;
        }
    }
}
