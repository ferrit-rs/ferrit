//! Hardcoded sample data.
//!
//! Phase 2 wired Status and Files to a real `git::Repo`; the mock versions of
//! those two now feed `App::mock()`, the render tests' repo-free path. Branches,
//! Commits and Stash stay mock until G3..G5. Values mirror the target screen in
//! `docs/PLAN_1_LAYOUT.md`.

use std::path::PathBuf;

use crate::git::{Change, FileEntry, StatusHeader};

/// Left pane 1, repo-free: the canonical status header.
pub fn mock_header() -> StatusHeader {
    StatusHeader {
        branch: "main".to_string(),
        detached: false,
        upstream: Some("origin/main".to_string()),
        ahead: 2,
        behind: 0,
        conflicts: 0,
    }
}

/// Left pane 2, repo-free: 1 modified, 1 untracked, 1 staged.
pub fn mock_files() -> Vec<FileEntry> {
    vec![
        FileEntry {
            path: PathBuf::from("src/main.rs"),
            staged: Change::None,
            worktree: Change::Modified,
            binary: false,
        },
        FileEntry {
            path: PathBuf::from("docs/notes.md"),
            staged: Change::None,
            worktree: Change::Untracked,
            binary: false,
        },
        FileEntry {
            path: PathBuf::from("Cargo.lock"),
            staged: Change::Added,
            worktree: Change::None,
            binary: false,
        },
    ]
}

/// Left pane 3: local branches, `*` marks the checked-out one.
pub const BRANCHES: &[&str] = &["* main", "  feat/tui-skeleton", "  fix/parse-args"];

/// Left pane 4: recent commits, `<short-hash> <subject>`.
pub const COMMITS: &[&str] = &[
    "5e04050 docs: expand the layout plan",
    "23023d9 docs: add inspiration notes",
    "2f9bd4f docs: drop the arch stub",
    "d5bc03c chore: initial commit",
];

/// Left pane 5: stash entries. Empty in the canonical state.
pub const STASH: &[&str] = &[];

/// Bottom box: the commands a real run would have shelled out.
pub const COMMAND_LOG: &[&str] = &["$ git status --porcelain", "$ git diff src/main.rs"];

/// Bottom line: inert lazygit-style key labels. None of these do anything yet.
pub const KEYBAR: &str =
    " <space> stage  <c> commit  <P> push  <p> pull  <?> keybinds  <q> quit";

/// Right pane when Status is focused.
pub const RIGHT_STATUS: &str = "On branch main
Your branch is ahead of 'origin/main' by 2 commits.
  (use \"git push\" to publish your local commits)

nothing to commit, working tree clean";

/// Right pane when Files is focused.
pub const RIGHT_DIFF: &str = "diff --git a/src/main.rs b/src/main.rs
@@ -1,3 +1,7 @@
-fn main() {
+fn main() -> Result<()> {
+    let repo = git::open(\".\")?;
     println!(\"ferrit\");
+    Ok(())
 }";

/// Right pane when Branches is focused.
pub const RIGHT_LOG: &str = "* 5e04050 (HEAD -> main) docs: expand the layout plan
* 23023d9 docs: add inspiration notes
* 2f9bd4f docs: drop the arch stub
* d5bc03c chore: initial commit";

/// Right pane when Commits is focused.
pub const RIGHT_COMMIT: &str = "commit 5e04050
Author: The ferrit Authors
Date:   today

    docs: expand the layout plan

diff --git a/docs/PLAN_1_LAYOUT.md b/docs/PLAN_1_LAYOUT.md
@@ -1,2 +1,9 @@
-# Plan: phase 1
+# Plan: phase 1, layout only";

/// Right pane when Stash is focused.
pub const RIGHT_STASH: &str = "(no stash entries)";

/// Help overlay body, toggled with `?`.
pub const HELP: &str = "1 .. 5           focus that pane
Tab / Right      focus next pane
Shift-Tab / Left focus previous pane
j / Down         move selection down
k / Up           move selection up
?                toggle this help
q / Ctrl-c       quit";
