use std::time::Duration;

use ratatui_core::layout::Rect;

use super::easing::Easing;

/// Lifecycle phase of the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Closed,
    Opening,
    Open,
    Closing,
}

/// Owns overlay lifecycle, animation progress, and the computed inner area.
///
/// # State machine
///
/// ```text
/// Closed ──open()──▸ Opening ──tick──▸ Open
///   ▴                                    │
///   └──tick──◂ Closing ◂──close()────────┘
/// ```
///
/// With default duration (zero), `open()`/`close()` transition instantly.
/// Set [`with_duration`](Self::with_duration) for animated transitions.
/// Interrupting mid-animation reverses from the current progress.
#[derive(Debug, Clone)]
pub struct OverlayState {
    pub(crate) phase: Phase,
    pub(crate) overlay_rect: Option<Rect>,
    pub(crate) inner_area: Option<Rect>,
    pub(crate) progress: f32,
    pub(crate) duration: Duration,
    pub(crate) easing: Easing,
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

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

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

            // Reverse from current progress
            Phase::Closing => {
                self.phase = Phase::Opening;
            },

            Phase::Open | Phase::Opening => {},
        }
    }

    pub fn close(&mut self) {
        match self.phase {
            Phase::Open => {
                if self.duration.is_zero() {
                    self.phase = Phase::Closed;
                    self.progress = 0.0;
                    self.overlay_rect = None;
                    self.inner_area = None;
                } else {
                    self.phase = Phase::Closing;
                    self.progress = 1.0;
                }
            },

            // Reverse from current progress
            Phase::Opening => {
                self.phase = Phase::Closing;
            },

            Phase::Closed | Phase::Closing => {},
        }
    }

    pub fn toggle(&mut self) {
        match self.phase {
            Phase::Closed | Phase::Closing => self.open(),
            Phase::Open | Phase::Opening => self.close(),
        }
    }

    /// Advance animation by `elapsed` time. No-op when fully open, closed, or duration is zero.
    pub fn tick(&mut self, elapsed: Duration) {
        if self.duration.is_zero() {
            return;
        }

        let dt = elapsed.as_secs_f32() / self.duration.as_secs_f32();

        match self.phase {
            Phase::Opening => {
                self.progress = (self.progress + dt).min(1.0);

                if self.progress >= 1.0 {
                    self.phase = Phase::Open;
                }
            },

            Phase::Closing => {
                self.progress = (self.progress - dt).max(0.0);

                if self.progress <= 0.0 {
                    self.phase = Phase::Closed;
                    self.overlay_rect = None;
                    self.inner_area = None;
                }
            },

            Phase::Closed | Phase::Open => {},
        }
    }

    pub fn is_open(&self) -> bool {
        self.phase == Phase::Open
    }

    pub fn is_closed(&self) -> bool {
        self.phase == Phase::Closed
    }

    pub fn is_animating(&self) -> bool {
        matches!(self.phase, Phase::Opening | Phase::Closing)
    }

    /// Full overlay bounding rect, set during render. `None` when closed.
    ///
    /// Use this for hit-testing (e.g. click-outside-to-dismiss):
    ///
    /// ```rust,ignore
    /// if let Some(rect) = state.overlay_rect() {
    ///     if !rect.contains(click_position) {
    ///         state.close();
    ///     }
    /// }
    /// ```
    pub fn overlay_rect(&self) -> Option<Rect> {
        self.overlay_rect
    }

    /// Region available for body content (inside block chrome), set during render.
    /// `None` when closed.
    pub fn inner_area(&self) -> Option<Rect> {
        self.inner_area
    }

    /// Eased visibility fraction: 0.0 = fully hidden, 1.0 = fully revealed.
    pub(crate) fn visibility(&self) -> f32 {
        match self.phase {
            Phase::Closed => 0.0,
            Phase::Open => 1.0,
            Phase::Opening | Phase::Closing => self.easing.apply(self.progress),
        }
    }
}

impl Default for OverlayState {
    fn default() -> Self {
        Self::new()
    }
}
