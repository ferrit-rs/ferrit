//! New-branch, upstream-input and note popup state / key handling.

use ratatui::crossterm::event::KeyCode;

/// Rows a `PageUp` / `PageDown` moves the command log viewer.
pub(crate) const COMMAND_LOG_PAGE: usize = 10;

/// The viewer's scroll offset (rows up from the newest entry) after `key`.
/// `usize::MAX` means "as far up as it goes"; the renderer clamps it.
pub(crate) fn scrolled_command_log(from_bottom: usize, key: KeyCode) -> usize {
    match key {
        KeyCode::Char('k') | KeyCode::Up => from_bottom.saturating_add(1),
        KeyCode::Char('j') | KeyCode::Down => from_bottom.saturating_sub(1),
        KeyCode::PageUp => from_bottom.saturating_add(COMMAND_LOG_PAGE),
        KeyCode::PageDown => from_bottom.saturating_sub(COMMAND_LOG_PAGE),
        KeyCode::Home | KeyCode::Char('g') => usize::MAX,
        KeyCode::End | KeyCode::Char('G') => 0,
        _ => from_bottom,
    }
}

#[derive(Clone, Copy)]
pub(crate) enum PopupKind {
    Commit,
    CommitAllConfirm,
    NewBranch,
    Stash,
    Name,
    CommandLog,
    Menu,
    Upstream,
    Askpass,
    CreateRemote,
    Note,
}

#[cfg(test)]
mod tests {
    use super::{COMMAND_LOG_PAGE, KeyCode, scrolled_command_log};

    #[test]
    fn k_and_j_move_one_row_and_stop_at_the_newest() {
        assert_eq!(scrolled_command_log(3, KeyCode::Char('k')), 4);
        assert_eq!(scrolled_command_log(3, KeyCode::Char('j')), 2);
        assert_eq!(scrolled_command_log(0, KeyCode::Char('j')), 0);
    }

    #[test]
    fn pages_jump_and_the_ends_snap() {
        assert_eq!(scrolled_command_log(0, KeyCode::PageUp), COMMAND_LOG_PAGE);
        assert_eq!(scrolled_command_log(4, KeyCode::PageDown), 0);
        assert_eq!(scrolled_command_log(7, KeyCode::Char('g')), usize::MAX);
        assert_eq!(scrolled_command_log(7, KeyCode::Char('G')), 0);
    }

    #[test]
    fn scrolling_up_from_the_far_end_does_not_wrap() {
        assert_eq!(
            scrolled_command_log(usize::MAX, KeyCode::Char('k')),
            usize::MAX
        );
    }

    #[test]
    fn other_keys_leave_the_offset_alone() {
        assert_eq!(scrolled_command_log(5, KeyCode::Char('x')), 5);
    }
}
