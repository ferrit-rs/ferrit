/// Focus index for a finite set of controls.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusRing {
    len: usize,
    selected: usize,
}

impl FocusRing {
    pub fn new(len: usize) -> Self {
        Self { len, selected: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn selected(&self) -> Option<usize> {
        (self.len > 0).then_some(self.selected)
    }

    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        self.selected = self.selected.min(len.saturating_sub(1));
    }

    pub fn select(&mut self, index: usize) {
        if self.len > 0 {
            self.selected = index.min(self.len - 1);
        }
    }

    pub fn next(&mut self) {
        if self.len > 0 {
            self.selected = (self.selected + 1) % self.len;
        }
    }

    pub fn previous(&mut self) {
        if self.len > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.len - 1);
        }
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.selected() == Some(index)
    }
}

#[cfg(test)]
mod tests {
    use crate::state::focus::FocusRing;

    #[test]
    fn wraps_in_both_directions() {
        let mut focus = FocusRing::new(2);
        focus.previous();
        assert_eq!(focus.selected(), Some(1));
        focus.next();
        assert_eq!(focus.selected(), Some(0));
    }

    #[test]
    fn empty_ring_has_no_selection() {
        let mut focus = FocusRing::new(0);
        focus.next();
        focus.previous();
        assert_eq!(focus.selected(), None);
    }
}
