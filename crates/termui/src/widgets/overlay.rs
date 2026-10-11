use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Color;
use ratatui::widgets::{Block, StatefulWidget, Widget};

/// 9-point positioning grid for overlay placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// Edge the overlay enters from during animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slide {
    Left,
    Right,
    Top,
    Bottom,
}

/// Animation curve applied to slide progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
}

impl Easing {
    pub fn apply(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            Self::EaseIn => progress * progress,
            Self::EaseOut => (1.0 - progress).mul_add(-(1.0 - progress), 1.0),
            Self::EaseInOut => {
                (2.0 * progress * progress).mul_add(-progress, 3.0 * progress * progress)
            },
        }
    }
}

/// Lifecycle phase of an overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Closed,
    Opening,
    Open,
    Closing,
}

/// Owns overlay lifecycle, animation progress, and computed hit-test areas.
#[derive(Clone, Debug)]
pub struct OverlayState {
    phase: Phase,
    overlay_rect: Option<Rect>,
    inner_area: Option<Rect>,
    progress: f32,
    duration: Duration,
    easing: Easing,
}

impl OverlayState {
    pub fn new() -> Self {
        Self {
            phase: Phase::Closed,
            overlay_rect: None,
            inner_area: None,
            progress: 0.0,
            duration: Duration::ZERO,
            easing: Easing::EaseOut,
        }
    }

    #[must_use]
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    #[must_use]
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    pub fn open(&mut self) {
        match self.phase {
            Phase::Closed => {
                if self.duration.is_zero() {
                    self.phase = Phase::Open;
                    self.progress = 1.0;
                } else {
                    self.phase = Phase::Opening;
                    self.progress = 0.0;
                }
            },
            Phase::Closing => self.phase = Phase::Opening,
            Phase::Open | Phase::Opening => {},
        }
    }

    pub fn close(&mut self) {
        match self.phase {
            Phase::Open => {
                if self.duration.is_zero() {
                    self.set_closed();
                } else {
                    self.phase = Phase::Closing;
                    self.progress = 1.0;
                }
            },
            Phase::Opening => self.phase = Phase::Closing,
            Phase::Closed | Phase::Closing => {},
        }
    }

    pub fn toggle(&mut self) {
        match self.phase {
            Phase::Closed | Phase::Closing => self.open(),
            Phase::Open | Phase::Opening => self.close(),
        }
    }

    pub fn tick(&mut self, elapsed: Duration) {
        if self.duration.is_zero() {
            return;
        }
        let delta = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        match self.phase {
            Phase::Opening => {
                self.progress = (self.progress + delta).min(1.0);
                if self.progress >= 1.0 {
                    self.phase = Phase::Open;
                }
            },
            Phase::Closing => {
                self.progress = (self.progress - delta).max(0.0);
                if self.progress <= 0.0 {
                    self.set_closed();
                }
            },
            Phase::Closed | Phase::Open => {},
        }
    }

    pub const fn phase(&self) -> Phase {
        self.phase
    }

    pub const fn is_open(&self) -> bool {
        matches!(self.phase, Phase::Open)
    }

    pub const fn is_closing(&self) -> bool {
        matches!(self.phase, Phase::Closing)
    }

    pub const fn is_closed(&self) -> bool {
        matches!(self.phase, Phase::Closed)
    }

    pub const fn is_animating(&self) -> bool {
        matches!(self.phase, Phase::Opening | Phase::Closing)
    }

    pub const fn overlay_rect(&self) -> Option<Rect> {
        self.overlay_rect
    }

    pub const fn inner_area(&self) -> Option<Rect> {
        self.inner_area
    }

    fn visibility(&self) -> f32 {
        match self.phase {
            Phase::Closed => 0.0,
            Phase::Open => 1.0,
            Phase::Opening | Phase::Closing => self.easing.apply(self.progress),
        }
    }

    fn set_closed(&mut self) {
        self.phase = Phase::Closed;
        self.progress = 0.0;
        self.overlay_rect = None;
        self.inner_area = None;
    }
}

impl Default for OverlayState {
    fn default() -> Self {
        Self::new()
    }
}

/// Dimming layer behind an open overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backdrop {
    base: Color,
    fg: Option<Color>,
}

impl Backdrop {
    pub fn new(base: Color) -> Self {
        Self { base, fg: None }
    }

    #[must_use]
    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    fn apply(self, buffer: &mut Buffer, area: Rect, exclude: Rect) {
        let fg = self.fg.unwrap_or_else(|| derive_fg(self.base));
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if exclude.contains((x, y).into()) {
                    continue;
                }
                let cell = &mut buffer[(x, y)];
                cell.fg = fg;
                cell.bg = self.base;
            }
        }
    }
}

/// Composable overlay chrome. Body content renders into the state's inner area.
#[derive(Clone, Debug)]
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

    #[must_use]
    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    #[must_use]
    pub fn width(mut self, width: Constraint) -> Self {
        self.width = width;
        self
    }

    #[must_use]
    pub fn height(mut self, height: Constraint) -> Self {
        self.height = height;
        self
    }

    #[must_use]
    pub fn offset(mut self, x: i16, y: i16) -> Self {
        self.offset = (x, y);
        self
    }

    #[must_use]
    pub fn slide(mut self, slide: Slide) -> Self {
        self.slide = Some(slide);
        self
    }

    #[must_use]
    pub fn backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = Some(backdrop);
        self
    }

    #[must_use]
    pub fn backdrop_option(mut self, backdrop: Option<Backdrop>) -> Self {
        self.backdrop = backdrop;
        self
    }

    #[must_use]
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    #[must_use]
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
        if state.is_closed() {
            state.overlay_rect = None;
            state.inner_area = None;
            return;
        }
        let area = area.intersection(buf.area);
        let full_rect = resolve_rect(area, self.width, self.height, self.anchor, self.offset);
        let overlay_rect = clip_by_slide(full_rect, self.slide, state.visibility());
        if overlay_rect.width == 0 || overlay_rect.height == 0 {
            state.overlay_rect = None;
            state.inner_area = None;
            return;
        }

        if let Some(backdrop) = self.backdrop {
            backdrop.apply(buf, area, overlay_rect);
        }
        clear_region(buf, overlay_rect);
        if let Some(color) = self.bg {
            fill_bg(buf, overlay_rect, color);
        }

        let inner = match self.block {
            Some(block) => {
                let inner = block.inner(overlay_rect);
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

fn resolve_rect(
    parent: Rect,
    width: Constraint,
    height: Constraint,
    anchor: Anchor,
    offset: (i16, i16),
) -> Rect {
    let width = resolve_constraint(width, parent.width).min(parent.width);
    let height = resolve_constraint(height, parent.height).min(parent.height);
    let (origin_x, origin_y) = match anchor {
        Anchor::TopLeft => (parent.x, parent.y),
        Anchor::Top => (parent.x + (parent.width - width) / 2, parent.y),
        Anchor::TopRight => (parent.x + parent.width - width, parent.y),
        Anchor::Left => (parent.x, parent.y + (parent.height - height) / 2),
        Anchor::Center => (
            parent.x + (parent.width - width) / 2,
            parent.y + (parent.height - height) / 2,
        ),
        Anchor::Right => (
            parent.x + parent.width - width,
            parent.y + (parent.height - height) / 2,
        ),
        Anchor::BottomLeft => (parent.x, parent.y + parent.height - height),
        Anchor::Bottom => (
            parent.x + (parent.width - width) / 2,
            parent.y + parent.height - height,
        ),
        Anchor::BottomRight => (
            parent.x + parent.width - width,
            parent.y + parent.height - height,
        ),
    };
    let x = shift_clamped(
        origin_x,
        offset.0,
        parent.x,
        parent.x + parent.width - width,
    );
    let y = shift_clamped(
        origin_y,
        offset.1,
        parent.y,
        parent.y + parent.height - height,
    );
    Rect::new(x, y, width, height)
}

fn resolve_constraint(constraint: Constraint, available: u16) -> u16 {
    match constraint {
        Constraint::Percentage(percent) => scale(available, u32::from(percent), 100),
        Constraint::Length(length) => length,
        Constraint::Min(minimum) => available.max(minimum),
        Constraint::Max(maximum) => available.min(maximum),
        Constraint::Ratio(numerator, denominator) => scale(available, numerator, denominator),
        Constraint::Fill(_) => available,
    }
}

fn scale(available: u16, numerator: u32, denominator: u32) -> u16 {
    let scaled = (u64::from(available) * u64::from(numerator))
        .checked_div(u64::from(denominator))
        .unwrap_or(0);
    u16::try_from(scaled).unwrap_or(u16::MAX)
}

fn shift_clamped(origin: u16, offset: i16, minimum: u16, maximum: u16) -> u16 {
    let shifted = i32::from(origin) + i32::from(offset);
    u16::try_from(shifted.clamp(i32::from(minimum), i32::from(maximum))).unwrap_or(maximum)
}

fn clip_by_slide(rect: Rect, slide: Option<Slide>, visibility: f32) -> Rect {
    let Some(slide) = slide.filter(|_| visibility < 1.0) else {
        return rect;
    };
    match slide {
        Slide::Right => {
            let width = visible_cells(rect.width, visibility);
            Rect::new(
                rect.right().saturating_sub(width),
                rect.y,
                width,
                rect.height,
            )
        },
        Slide::Left => Rect::new(
            rect.x,
            rect.y,
            visible_cells(rect.width, visibility),
            rect.height,
        ),
        Slide::Bottom => {
            let height = visible_cells(rect.height, visibility);
            Rect::new(
                rect.x,
                rect.bottom().saturating_sub(height),
                rect.width,
                height,
            )
        },
        Slide::Top => Rect::new(
            rect.x,
            rect.y,
            rect.width,
            visible_cells(rect.height, visibility),
        ),
    }
}

fn visible_cells(extent: u16, visibility: f32) -> u16 {
    let target = visibility.mul_add(f32::from(extent), 0.5);
    (0..=extent)
        .rev()
        .find(|&cells| f32::from(cells) <= target)
        .unwrap_or(0)
}

fn clear_region(buffer: &mut Buffer, area: Rect) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buffer[(x, y)].reset();
        }
    }
}

fn fill_bg(buffer: &mut Buffer, area: Rect, color: Color) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buffer[(x, y)].bg = color;
        }
    }
}

fn derive_fg(base: Color) -> Color {
    let (red, green, blue) = match base {
        Color::Rgb(red, green, blue) => (red, green, blue),
        Color::Reset => (0, 0, 0),
        Color::Black => (0, 0, 0),
        Color::Red => (128, 0, 0),
        Color::Green => (0, 128, 0),
        Color::Yellow => (128, 128, 0),
        Color::Blue => (0, 0, 128),
        Color::Magenta => (128, 0, 128),
        Color::Cyan => (0, 128, 128),
        Color::Gray => (192, 192, 192),
        Color::DarkGray => (128, 128, 128),
        Color::LightRed => (255, 0, 0),
        Color::LightGreen => (0, 255, 0),
        Color::LightYellow => (255, 255, 0),
        Color::LightBlue => (0, 0, 255),
        Color::LightMagenta => (255, 0, 255),
        Color::LightCyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        Color::Indexed(index) => indexed_rgb(index),
    };
    Color::Rgb(red / 2 + 64, green / 2 + 64, blue / 2 + 64)
}

fn indexed_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0 => (0, 0, 0),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        16..=231 => {
            let index = index - 16;
            let channel = |value: u8| if value == 0 { 0 } else { 55 + 40 * value };
            (
                channel(index / 36),
                channel((index % 36) / 6),
                channel(index % 6),
            )
        },
        232..=255 => {
            let gray = 8 + 10 * (index - 232);
            (gray, gray, gray)
        },
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ratatui::layout::Constraint;

    use crate::widgets::overlay::{Anchor, Easing, Overlay, OverlayState, Phase, Slide};

    #[test]
    fn state_opens_and_closes_without_duration() {
        let mut state = OverlayState::new();
        state.open();
        assert_eq!(state.phase(), Phase::Open);
        state.close();
        assert!(state.is_closed());
    }

    #[test]
    fn state_animates_then_reaches_open() {
        let mut state = OverlayState::new()
            .with_duration(Duration::from_millis(100))
            .with_easing(Easing::Linear);
        state.open();
        state.tick(Duration::from_millis(100));
        assert!(state.is_open());
    }

    #[test]
    fn overlay_builder_accepts_sheet_configuration() {
        let _ = Overlay::new()
            .anchor(Anchor::Right)
            .slide(Slide::Right)
            .width(Constraint::Percentage(40))
            .height(Constraint::Percentage(100));
    }

    #[test]
    fn easing_stays_in_range() {
        assert!((0.0..=1.0).contains(&Easing::EaseOut.apply(0.5)));
    }
}
