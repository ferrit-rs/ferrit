//! Write the index (and, for discard, the worktree) by piping a patch built
//! from `Diff` byte ranges to `git apply`, or by shelling out to `git add` /
//! `restore` / `clean` for whole-file moves. See `docs/PLAN_6_STAGING.md`.
//!
//! Same rule as the rest of `git::`: no `ratatui`, `git` does the applying
//! (fuzz, whitespace policy, `core.autocrlf` are its problem, not ours), one
//! subprocess per action, no long-lived patch-builder state.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::infra::git::apply`.

use std::collections::HashSet;

/// Which way a patch runs: stage / discard read forward, unstage reads
/// `git apply --reverse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyDir {
    Forward,
    Reverse,
}

/// What the patch touches. `Index` alone is stage/unstage; `Worktree` /
/// `WorktreeAndIndex` are discard (the worktree side, optionally keeping the
/// index in step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyTarget {
    Index,
    Worktree,
    WorktreeAndIndex,
}

/// Turn a hunk body into a valid, self-contained patch body covering only
/// `lines` (0-based indices into `body`'s own lines), gitu's
/// `format_line_patch` rule. Exposed for `tests/apply_patch.rs`, which checks
/// it byte-for-byte with no repo involved.
///
/// The same rule serves staging and unstaging: `git apply --reverse` (used
/// by unstage and by a worktree discard) flips which side of the patch is
/// "old" and which is "new", so a selected line kept verbatim always means
/// "this exact hunk line moves" in whichever direction is asked for, and an
/// unselected `+` dropped / unselected `-` demoted to context always means
/// "this line is untouched" on the side that has to stay fixed either way.
/// Only the CLI flag differs between the two directions, not this transform.
pub fn transform_body(body: &str, lines: &[usize]) -> String {
    let selected: HashSet<usize> = lines.iter().copied().collect();
    let mut out = String::with_capacity(body.len());
    // Did the immediately preceding non-`\` line survive the transform? A
    // trailing `\ No newline at end of file` marker is not itself
    // selectable: it rides along with the line above, dropped with it if
    // that line was an unselected `+`.
    let mut last_kept = true;
    for (i, line) in body.split_inclusive('\n').enumerate() {
        match line.as_bytes().first() {
            Some(b'+') if selected.contains(&i) => {
                out.push_str(line);
                last_kept = true;
            },
            Some(b'+') => last_kept = false,
            Some(b'-') if selected.contains(&i) => {
                out.push_str(line);
                last_kept = true;
            },
            Some(b'-') => {
                out.push(' ');
                out.push_str(line.get(1..).unwrap_or_default());
                last_kept = true;
            },
            Some(b'\\') => {
                if last_kept {
                    out.push_str(line);
                }
            },
            _ => {
                out.push_str(line);
                last_kept = true;
            },
        }
    }
    out
}
