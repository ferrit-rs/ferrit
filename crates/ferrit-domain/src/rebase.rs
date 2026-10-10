//! Rewrite history from the Commits pane: reword, drop, edit, squash or fixup
//! one commit, and fold pending `fixup!` commits, each as one
//! `git rebase -i` run with a todo ferrit generates. See
//! `docs/PLAN_11_REBASE.md`.
//!
//! ferrit never opens `$EDITOR` (the terminal is in raw mode on an alternate
//! screen). The todo is supplied through `GIT_SEQUENCE_EDITOR="cp <file>"` and
//! every other editor is `GIT_EDITOR=true`.
//!
//! A reword is a `pick` followed by `exec git commit --amend -F <file>`, not a
//! `reword` line: the exec line lives in git's own copy of the todo, so it
//! still runs after the rebase stopped on a conflict earlier in the range and
//! was continued with a neutral editor. The message file therefore outlives
//! the run that created it; it is removed once the rebase is done.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::git::repo::rebase`.

use crate::model::CommitEntry;
use std::path::Path;

/// What to do with the selected commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseEdit {
    /// Replace its message (the new one, whole).
    Reword(String),
    /// Remove it from history.
    Drop,
    /// Stop at it, leaving the rebase in progress.
    Edit,
    /// Fold into the commit below it, keeping both messages.
    Squash,
    /// Fold into the commit below it, dropping this message.
    Fixup,
}

/// The todo for `commits` (full hashes, oldest first) with `target` carrying
/// `edit` and every other commit picked. A reword is a `pick` plus an `exec`
/// amend reading `message_file`.
pub fn build_todo(
    commits: &[String],
    target: &str,
    edit: &RebaseEdit,
    message_file: &Path,
) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(commits.len() + 1);
    for hash in commits {
        if hash != target {
            lines.push(format!("pick {hash}"));
            continue;
        }
        match edit {
            RebaseEdit::Reword(_) => {
                lines.push(format!("pick {hash}"));
                lines.push(format!(
                    "exec git commit --amend -q --allow-empty -F {}",
                    shell_quote(&message_file.to_string_lossy())
                ));
            },
            RebaseEdit::Drop => lines.push(format!("drop {hash}")),
            RebaseEdit::Edit => lines.push(format!("edit {hash}")),
            RebaseEdit::Squash => lines.push(format!("squash {hash}")),
            RebaseEdit::Fixup => lines.push(format!("fixup {hash}")),
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    let mut todo = lines.join("\n");
    todo.push('\n');
    todo
}

/// `text` as one shell word, safe for a path containing spaces or quotes.
/// Quote text for a shell command embedded in a rebase instruction.
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Does any `fixup! <subject>` / `squash! <subject>` among `commits[..=selected]`
/// (newest first) have a commit with that subject further down, still inside
/// the range? Only then does an autosquash from `selected` change anything.
#[must_use]
pub fn has_foldable_fixup(commits: &[CommitEntry], selected: usize) -> bool {
    let Some(range) = commits.get(..=selected) else {
        return false;
    };
    range.iter().enumerate().any(|(index, commit)| {
        let Some(target) = commit
            .summary
            .strip_prefix("fixup! ")
            .or_else(|| commit.summary.strip_prefix("squash! "))
        else {
            return false;
        };
        range
            .iter()
            .skip(index + 1)
            .any(|candidate| candidate.summary == target)
    })
}

/// What the user asks of a stopped operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Carry on after the user resolved what stopped it.
    Continue,
    /// Not available for a merge: git has no `merge --skip`.
    Skip,
    /// Give up and return to where the operation started.
    Abort,
}

/// Where the repository is after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationOutcome {
    /// No operation in progress any more.
    Done,
    /// Git stopped again and waits for the user: on a conflict, or, for a
    /// rebase, at an `edit` step.
    Stopped {
        /// The index has unresolved conflicts.
        conflicted: bool,
    },
}

/// The Git subcommand used to continue a stopped operation.
pub fn flag(step: Step) -> &'static str {
    match step {
        Step::Continue => "--continue",
        Step::Skip => "--skip",
        Step::Abort => "--abort",
    }
}
