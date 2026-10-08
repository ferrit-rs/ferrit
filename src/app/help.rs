//! The `?` help screen: whether it is up, its scroll, and its `/` search. The
//! key text it shows comes from `app::hints`; this is only its state.

use std::time::Duration;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::components::tui_overlay::state::OverlayState;
use crate::components::ui::text_input::{TextInput, TextInputMode};

/// Whether keys move through the help or type into its search box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelpMode {
    Browse,
    Search,
}

pub struct HelpState {
    /// The help is up (it may still be sliding out: see `is_visible`).
    pub open: bool,
    scroll: usize,
    /// Rows the last frame showed, so a page scrolls by what is on screen.
    rows: usize,
    query: TextInput,
    mode: HelpMode,
    pub(crate) overlay: OverlayState,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            open: false,
            scroll: 0,
            rows: 0,
            query: TextInput::default(),
            mode: HelpMode::Browse,
            overlay: OverlayState::new().with_duration(Duration::from_millis(180)),
        }
    }
}

impl HelpState {
    /// Up, or still animating out.
    pub(crate) fn is_visible(&self) -> bool {
        self.open || !self.overlay.is_closed()
    }

    /// Show the help from the top, with an empty search.
    pub(crate) fn show(&mut self) {
        self.open = true;
        self.reset();
    }

    /// Hide it, and start the slide-out.
    pub(crate) fn dismiss(&mut self) {
        self.open = false;
        self.reset();
        self.overlay.close();
    }

    fn reset(&mut self) {
        self.scroll = 0;
        self.query = TextInput::default();
        self.mode = HelpMode::Browse;
    }

    /// What drawing needs, borrowed field by field so the overlay can be
    /// `&mut` while the query is read: scroll, query, searching, overlay.
    pub(crate) fn view_parts(&mut self) -> (usize, &TextInput, bool, &mut OverlayState) {
        (
            self.scroll,
            &self.query,
            self.mode == HelpMode::Search,
            &mut self.overlay,
        )
    }

    pub(crate) fn query(&self) -> &TextInput {
        &self.query
    }

    pub(crate) fn is_searching(&self) -> bool {
        self.mode == HelpMode::Search
    }

    pub(crate) fn set_rows(&mut self, rows: usize) {
        self.rows = rows;
    }

    /// A key while the search box has the keyboard.
    pub(super) fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => self.mode = HelpMode::Browse,
            _ if self.query.handle_key_event(key, TextInputMode::SingleLine) => {
                self.scroll = 0;
            },
            _ => {},
        }
    }

    pub(super) fn start_search(&mut self) {
        self.query = TextInput::default();
        self.mode = HelpMode::Search;
        self.scroll = 0;
    }

    /// Scroll for `code` over `total` filtered lines; `false` for any other key.
    pub(super) fn scroll_key(&mut self, code: KeyCode, total: usize) -> bool {
        let max = total.saturating_sub(self.rows.max(1));
        let page = self.rows.saturating_sub(1).max(1);
        match code {
            KeyCode::Char('j') | KeyCode::Down => self.scroll = (self.scroll + 1).min(max),
            KeyCode::Char('k') | KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = (self.scroll + page).min(max),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(page),
            KeyCode::Home | KeyCode::Char('g') => self.scroll = 0,
            KeyCode::End | KeyCode::Char('G') => self.scroll = max,
            _ => return false,
        }
        true
    }
}
