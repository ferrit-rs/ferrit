/// Small scroll model for list-like terminal components.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollState {
    offset: usize,
    content_len: usize,
    viewport_len: usize,
}

impl ScrollState {
    pub const fn new(content_len: usize, viewport_len: usize) -> Self {
        Self {
            offset: 0,
            content_len,
            viewport_len,
        }
    }

    pub const fn offset(self) -> usize {
        self.offset
    }

    pub const fn content_len(self) -> usize {
        self.content_len
    }

    pub const fn viewport_len(self) -> usize {
        self.viewport_len
    }

    pub fn set_content_len(&mut self, content_len: usize) {
        self.content_len = content_len;
        self.clamp_offset();
    }

    pub fn set_viewport_len(&mut self, viewport_len: usize) {
        self.viewport_len = viewport_len;
        self.clamp_offset();
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.offset = self.offset.saturating_add(amount);
        self.clamp_offset();
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.offset = self.offset.saturating_sub(amount);
    }

    pub fn page_down(&mut self) {
        self.scroll_down(self.viewport_len.max(1));
    }

    pub fn page_up(&mut self) {
        self.scroll_up(self.viewport_len.max(1));
    }

    pub fn ensure_visible(&mut self, index: usize) {
        if index < self.offset {
            self.offset = index;
        } else if index >= self.offset.saturating_add(self.viewport_len) {
            self.offset = index
                .saturating_add(1)
                .saturating_sub(self.viewport_len.max(1));
        }
        self.clamp_offset();
    }

    fn clamp_offset(&mut self) {
        self.offset = self
            .offset
            .min(self.content_len.saturating_sub(self.viewport_len));
    }
}

#[cfg(test)]
mod tests {
    use crate::state::scroll::ScrollState;

    #[test]
    fn scroll_state_clamps_at_content_end() {
        let mut state = ScrollState::new(10, 3);
        state.scroll_down(100);
        assert_eq!(state.offset(), 7);
    }

    #[test]
    fn ensure_visible_moves_both_directions() {
        let mut state = ScrollState::new(20, 4);
        state.scroll_down(8);
        state.ensure_visible(2);
        assert_eq!(state.offset(), 2);
        state.ensure_visible(10);
        assert_eq!(state.offset(), 7);
    }
}
