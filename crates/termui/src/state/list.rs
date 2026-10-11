use crate::state::focus::FocusRing;
use crate::state::scroll::ScrollState;

/// Selection and viewport state for list-like components.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListState {
    focus: FocusRing,
    scroll: ScrollState,
}

impl ListState {
    pub fn new(len: usize, viewport_len: usize) -> Self {
        Self {
            focus: FocusRing::new(len),
            scroll: ScrollState::new(len, viewport_len),
        }
    }

    pub fn set_len(&mut self, len: usize) {
        self.focus.set_len(len);
        self.scroll.set_content_len(len);
        self.ensure_selected_visible();
    }

    pub fn set_viewport_len(&mut self, viewport_len: usize) {
        self.scroll.set_viewport_len(viewport_len);
        self.ensure_selected_visible();
    }

    pub fn len(&self) -> usize {
        self.focus.len()
    }

    pub fn is_empty(&self) -> bool {
        self.focus.is_empty()
    }

    pub fn viewport_len(&self) -> usize {
        self.scroll.viewport_len()
    }

    pub const fn offset(&self) -> usize {
        self.scroll.offset()
    }

    pub fn selected(&self) -> Option<usize> {
        self.focus.selected()
    }

    pub fn select(&mut self, index: usize) {
        self.focus.select(index);
        self.ensure_selected_visible();
    }

    pub fn next(&mut self) {
        self.focus.next();
        self.ensure_selected_visible();
    }

    pub fn previous(&mut self) {
        self.focus.previous();
        self.ensure_selected_visible();
    }

    pub fn page_down(&mut self) {
        if let Some(selected) = self.selected() {
            self.focus
                .select(selected.saturating_add(self.viewport_len().max(1)));
        }
        self.ensure_selected_visible();
    }

    pub fn page_up(&mut self) {
        if let Some(selected) = self.selected() {
            self.focus
                .select(selected.saturating_sub(self.viewport_len().max(1)));
        }
        self.ensure_selected_visible();
    }

    fn ensure_selected_visible(&mut self) {
        if let Some(selected) = self.selected() {
            self.scroll.ensure_visible(selected);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::list::ListState;

    #[test]
    fn selection_keeps_viewport_in_sync() {
        let mut state = ListState::new(20, 4);
        state.select(10);
        assert_eq!(state.selected(), Some(10));
        assert_eq!(state.offset(), 7);
        state.previous();
        assert_eq!(state.selected(), Some(9));
        assert_eq!(state.offset(), 7);
    }

    #[test]
    fn page_movement_selects_visible_edge() {
        let mut state = ListState::new(20, 4);
        state.page_down();
        assert_eq!(state.selected(), Some(4));
        assert_eq!(state.offset(), 1);
        state.page_down();
        assert_eq!(state.selected(), Some(8));
        assert_eq!(state.offset(), 5);
        state.page_up();
        assert_eq!(state.selected(), Some(4));
        assert_eq!(state.offset(), 4);
    }
}
