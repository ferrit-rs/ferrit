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
//! `git2` or runs `git` is `crate::infra::git::rebase`.

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
pub(crate) fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
