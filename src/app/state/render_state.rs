//! What ratatui needs mutable to show the app, as opposed to the app itself:
//! the slide-in animation of the help, of the side sheet and of the commit
//! popup, and the error toast (which owns its own animation). Key handlers
//! open and close them and the run loop ticks them; drawing renders through
//! them. They are kept apart so drawing can read `&App` and take only these as
//! `&mut` (`docs/PLAN_24_DRAW_VIEW.md`).

use crate::git::image::preview::Preview;
use ratatui::text::Text;
use std::ops::Range;
use std::time::Duration;

use ratatui::crossterm::event::MouseEvent;

use crate::app::state::diff_query::RightKey;
use crate::ui::widgets::toast::Toast;
use crate::ui::widgets::tui_overlay::state::OverlayState;

pub(crate) struct RenderState {
    /// The help dialog.
    pub(crate) help: OverlayState,
    /// The side drawer, whichever sheet it holds.
    pub(crate) sheet: OverlayState,
    /// The backdrop of the commit editor and of the "stage everything?" question.
    pub(crate) commit: OverlayState,
    /// The bottom-right error notification, dismissed by its `x`, `Esc` or a timeout.
    pub(crate) toast: Option<Toast>,
    /// The right pane's image: a live protocol that resizes and re-encodes itself
    /// at render time, through `&mut`.
    pub(crate) preview: Preview,
    /// The styled commit diff, kept so scrolling does not rerun syntax
    /// highlighting. Keyed by the selection, the diff text, the focus range and
    /// the pane width.
    pub(crate) diff_cache: Option<RenderedDiff>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            help: OverlayState::new().with_duration(Duration::from_millis(180)),
            sheet: OverlayState::new().with_duration(Duration::from_millis(200)),
            commit: OverlayState::new(),
            toast: None,
            preview: Preview::None,
            diff_cache: None,
        }
    }
}

pub(crate) struct RenderedDiff {
    pub(crate) key: Option<RightKey>,
    pub(crate) source: String,
    pub(crate) focus: Option<Range<usize>>,
    pub(crate) width: usize,
    pub(crate) text: Text<'static>,
}

impl RenderState {
    /// Count the error toast's timeout and animation, and drop it once closed.
    pub(crate) fn tick_toast(&mut self, elapsed: Duration) {
        if let Some(toast) = &mut self.toast {
            toast.tick(elapsed);
            if toast.is_closed() {
                self.toast = None;
            }
        }
    }

    /// Start closing the toast. `true` when there was one to close.
    pub(crate) fn dismiss_toast(&mut self) -> bool {
        match &mut self.toast {
            Some(toast) if !toast.is_closing() => {
                toast.dismiss();
                true
            },
            _ => false,
        }
    }
}

/// Which of the animated things were moving at one instant.
#[derive(Clone, Copy)]
pub(crate) struct Animating {
    sheet: bool,
    help: bool,
    toast: bool,
}

impl Animating {
    pub(crate) const fn any(self) -> bool {
        self.sheet || self.help || self.toast
    }
}

impl RenderState {
    pub(crate) fn animating(&self) -> Animating {
        Animating {
            sheet: self.sheet.is_animating(),
            help: self.help.is_animating(),
            toast: self.toast.as_ref().is_some_and(Toast::is_animating),
        }
    }

    /// Advance every animation and the toast's timeout by `elapsed`.
    pub(crate) fn tick(&mut self, elapsed: Duration) {
        self.sheet.tick(elapsed);
        self.help.tick(elapsed);
        self.tick_toast(elapsed);
    }

    /// The same at the end of a batch of events: a sheet or the help that began
    /// to animate during the batch waits for its first frame, so only the ones
    /// already moving (`was`) advance.
    pub(crate) fn tick_after_batch(&mut self, elapsed: Duration, was: Animating) {
        if self.sheet.is_animating() && was.sheet {
            self.sheet.tick(elapsed);
        }
        if self.help.is_animating() && was.help {
            self.help.tick(elapsed);
        }
        self.tick_toast(elapsed);
    }

    /// Give a mouse event to the toast. `true` when it consumed it.
    pub(crate) fn toast_mouse(&mut self, event: MouseEvent) -> bool {
        self.toast
            .as_mut()
            .is_some_and(|toast| toast.on_mouse(event))
    }
}
