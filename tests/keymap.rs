#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test: a failed setup or a bad slice is the assertion"
)]
//! The default keymap (`docs/PLAN_12_POLISH.md` P2a). `EXPECTED` was written
//! from the arms of `App::on_key` and `App::on_diff_key` as they were before
//! the keymap existed, not from `keymap::DEFAULTS`: a default that drifts from
//! the old behaviour fails here.

use ferrit::app::Pane;
use ferrit::app::keymap::{Action, Context, KeyBinding, Keymap};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use Action as A;
use Context as C;

/// `(context, key, action)` for every binding the old `on_key` had.
const EXPECTED: &[(Context, &str, Action)] = &[
    // The main `match key.code`, unguarded arms.
    (C::Global, "q", A::Quit),
    (C::Global, "?", A::Help),
    (C::Global, "@", A::CommandLog),
    (C::Global, "m", A::OperationMenu),
    (C::Global, "esc", A::Back),
    (C::Global, "enter", A::Enter),
    (C::Global, "l", A::EnterDiff),
    (C::Global, "r", A::Refresh),
    (C::Global, "f", A::Fetch),
    (C::Global, "p", A::Pull),
    (C::Global, "P", A::Push),
    (C::Global, "c", A::Commit),
    (C::Global, "A", A::Amend),
    (C::Global, "w", A::RewordHead),
    (C::Global, "1", A::Focus(Pane::Status)),
    (C::Global, "2", A::Focus(Pane::Files)),
    (C::Global, "3", A::Focus(Pane::Branches)),
    (C::Global, "4", A::Focus(Pane::Commits)),
    (C::Global, "5", A::Focus(Pane::Stash)),
    (C::Global, "tab", A::NextPane),
    (C::Global, "right", A::NextPane),
    (C::Global, "backtab", A::PrevPane),
    (C::Global, "left", A::PrevPane),
    (C::Global, "ctrl-right", A::ToggleBranchesTab),
    (C::Global, "ctrl-left", A::ToggleBranchesTab),
    (C::Global, "j", A::SelectDown),
    (C::Global, "down", A::SelectDown),
    (C::Global, "k", A::SelectUp),
    (C::Global, "up", A::SelectUp),
    // The right-pane scroll block that runs before it.
    (C::Global, "J", A::ScrollLineDown),
    (C::Global, "K", A::ScrollLineUp),
    (C::Global, "pgdn", A::ScrollPageDown),
    (C::Global, "pgup", A::ScrollPageUp),
    (C::Global, "ctrl-d", A::ScrollHalfDown),
    (C::Global, "ctrl-u", A::ScrollHalfUp),
    (C::Global, "<", A::ScrollTop),
    (C::Global, ">", A::ScrollBottom),
    (C::Global, "]", A::NextHunk),
    (C::Global, "[", A::PrevHunk),
    // Arms guarded by `self.focus == Pane::Files` (or ending in a Files-only
    // method: `stage_selected_file`, `stage_all_files`, `discard_prompt`).
    (C::Files, "space", A::StageFile),
    (C::Files, "a", A::StageAll),
    (C::Files, "d", A::Discard),
    (C::Files, "s", A::StashPush),
    // `on_diff_key`.
    (C::Diff, "esc", A::LeaveDiff),
    (C::Diff, "h", A::LeaveDiff),
    (C::Diff, "j", A::CursorDown),
    (C::Diff, "down", A::CursorDown),
    (C::Diff, "k", A::CursorUp),
    (C::Diff, "up", A::CursorUp),
    (C::Diff, "]", A::CursorNextHunk),
    (C::Diff, "[", A::CursorPrevHunk),
    (C::Diff, "V", A::ToggleSelection),
    (C::Diff, "space", A::StageCursor),
    (C::Diff, "d", A::Discard),
    // Arms guarded by `self.focus == Pane::Branches`.
    (C::Branches, "space", A::Checkout),
    (C::Branches, "n", A::NewBranch),
    (C::Branches, "u", A::FastForward),
    (C::Branches, "M", A::Merge),
    (C::Branches, "d", A::DeleteBranch),
    // Arms guarded by `self.focus == Pane::Commits`.
    (C::Commits, "w", A::RewordCommit),
    (C::Commits, "d", A::DropCommit),
    (C::Commits, "s", A::Squash),
    (C::Commits, "S", A::Fixup),
    (C::Commits, "e", A::EditCommit),
    (C::Commits, "F", A::NewFixup),
    (C::Commits, "a", A::Autosquash),
    // Arms guarded by `self.focus == Pane::Stash`.
    (C::Stash, "space", A::ApplyStash),
    (C::Stash, "g", A::PopStash),
    (C::Stash, "d", A::DropStash),
];

fn key(text: &str) -> KeyBinding {
    KeyBinding::parse(text).unwrap_or_else(|| panic!("{text:?} does not parse"))
}

#[test]
fn every_old_binding_resolves_to_its_action_in_its_own_context() {
    let map = Keymap::default();
    for &(context, text, action) in EXPECTED {
        assert_eq!(
            map.resolve(&[context], key(text)),
            Some(action),
            "{context:?} {text}"
        );
    }
}

#[test]
fn the_default_keymap_binds_nothing_the_old_code_did_not() {
    let map = Keymap::default();
    assert_eq!(map.bindings().count(), EXPECTED.len());
    for (context, binding, action) in map.bindings() {
        assert!(
            EXPECTED
                .iter()
                .any(|&(c, text, a)| c == context && key(text) == binding && a == action),
            "extra default: {context:?} {binding} -> {action:?}"
        );
    }
}

#[test]
fn a_context_is_tried_before_global_and_falls_through_when_it_has_no_binding() {
    let map = Keymap::default();
    // `d`: drop on Commits, delete on Branches, discard on Files and in the
    // diff cursor, apply-nothing on Status (unbound there).
    assert_eq!(
        map.resolve(&[C::Commits, C::Global], key("d")),
        Some(A::DropCommit)
    );
    assert_eq!(
        map.resolve(&[C::Branches, C::Global], key("d")),
        Some(A::DeleteBranch)
    );
    assert_eq!(
        map.resolve(&[C::Diff, C::Files, C::Global], key("d")),
        Some(A::Discard)
    );
    assert_eq!(map.resolve(&[C::Global], key("d")), None);
    // `q` is global: every pane falls through to it.
    assert_eq!(map.resolve(&[C::Stash, C::Global], key("q")), Some(A::Quit));
    // In the diff cursor `j` moves the cursor; outside it, the selection.
    assert_eq!(
        map.resolve(&[C::Diff, C::Files, C::Global], key("j")),
        Some(A::CursorDown)
    );
    assert_eq!(
        map.resolve(&[C::Files, C::Global], key("j")),
        Some(A::SelectDown)
    );
    // A key the diff cursor does not bind falls through to the scroll block.
    assert_eq!(
        map.resolve(&[C::Diff, C::Files, C::Global], key("J")),
        Some(A::ScrollLineDown)
    );
}

#[test]
fn modifiers_must_match_exactly() {
    let map = Keymap::default();
    let ctrl = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
    let alt = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT);
    let global = [C::Global];
    let files = [C::Files, C::Global];

    assert_eq!(
        map.resolve(&global, KeyBinding::from_event(ctrl('d'))),
        Some(A::ScrollHalfDown)
    );
    // Before the keymap, `Ctrl-p` pulled and `Ctrl-d` on Files opened the
    // discard prompt, because character keys ignored modifiers.
    assert_eq!(
        map.resolve(&global, KeyBinding::from_event(ctrl('p'))),
        None
    );
    assert_eq!(map.resolve(&files, KeyBinding::from_event(ctrl('a'))), None);
    assert_eq!(map.resolve(&global, KeyBinding::from_event(alt('q'))), None);
    assert_eq!(
        map.resolve(
            &global,
            KeyBinding::from_event(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL))
        ),
        Some(A::ToggleBranchesTab)
    );
    assert_eq!(
        map.resolve(
            &global,
            KeyBinding::from_event(KeyEvent::from(KeyCode::Right))
        ),
        Some(A::NextPane)
    );
}

#[test]
fn shift_is_carried_by_the_character_not_a_modifier() {
    let map = Keymap::default();
    let shifted = KeyEvent::new(KeyCode::Char('J'), KeyModifiers::SHIFT);
    assert_eq!(
        map.resolve(&[C::Global], KeyBinding::from_event(shifted)),
        Some(A::ScrollLineDown)
    );
    let back_tab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_eq!(
        map.resolve(&[C::Global], KeyBinding::from_event(back_tab)),
        Some(A::PrevPane)
    );
}

#[test]
fn every_default_key_text_round_trips_through_display() {
    for &(_, text, _) in EXPECTED {
        let binding = key(text);
        assert_eq!(
            KeyBinding::parse(&binding.to_string()),
            Some(binding),
            "{text}"
        );
    }
}

#[test]
fn key_text_parses_names_modifiers_and_characters() {
    assert_eq!(
        key("Ctrl-D"),
        key("ctrl-d"),
        "modifier and name are case-insensitive"
    );
    assert_eq!(key("ctrl-alt-x").to_string(), "ctrl-alt-x");
    assert_eq!(key("F5").code, KeyCode::F(5));
    assert_eq!(key("pageup"), key("pgup"));
    assert_eq!(key("shift-tab"), key("backtab"));
    assert_ne!(key("J"), key("j"), "a single character is taken as typed");
    assert_eq!(key(" ").code, KeyCode::Char(' '));
    assert_eq!(key("ctrl-right").code, KeyCode::Right);
    assert!(key("ctrl-right").ctrl);
}

#[test]
fn text_that_is_not_a_key_does_not_parse() {
    for bad in [
        "",
        "ctrl-",
        "ctrl-ctrl",
        "f0",
        "f13",
        "enterx",
        "super-x",
        "ctrl-alt-",
        "x y",
    ] {
        assert_eq!(KeyBinding::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn a_pane_maps_to_its_own_context() {
    assert_eq!(Context::for_pane(Pane::Status), None);
    assert_eq!(Context::for_pane(Pane::Files), Some(C::Files));
    assert_eq!(Context::for_pane(Pane::Branches), Some(C::Branches));
    assert_eq!(Context::for_pane(Pane::Commits), Some(C::Commits));
    assert_eq!(Context::for_pane(Pane::Stash), Some(C::Stash));
}
