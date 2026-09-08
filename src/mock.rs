//! Hardcoded sample data.
//!
//! Every pane now reads from a real `git::Repo`; this module only feeds
//! `App::mock()`, the render tests' repo-free path. Values mirror the target
//! screen in `docs/PLAN_1_LAYOUT.md`.

use std::path::{Path, PathBuf};

use crate::git::{
    BranchEntry, Change, CommitEntry, FileEntry, StashEntry, StatusHeader,
};

/// An 8x8 PNG, embedded so `App::mock()` can drive the image-preview path with
/// no repo and nothing on disk.
pub const LOGO_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 8, 8, 2, 0,
    0, 0, 75, 109, 41, 220, 0, 0, 0, 108, 73, 68, 65, 84, 120, 218, 21, 205, 65, 21, 0, 81, 8, 66,
    81, 163, 24, 133, 40, 70, 121, 81, 136, 66, 20, 162, 204, 31, 151, 92, 14, 206, 12, 59, 104,
    184, 129, 193, 67, 134, 14, 51, 203, 46, 90, 110, 97, 241, 146, 165, 251, 64, 172, 144, 56, 129,
    176, 136, 168, 30, 28, 123, 232, 184, 131, 195, 71, 142, 222, 131, 127, 224, 85, 95, 248, 159,
    33, 208, 247, 110, 204, 26, 153, 243, 31, 219, 196, 212, 15, 194, 6, 133, 203, 95, 118, 72, 104,
    30, 148, 45, 42, 215, 127, 194, 37, 165, 229, 3, 198, 123, 88, 1, 87, 57, 54, 242, 0, 0, 0, 0,
    73, 69, 78, 68, 174, 66, 96, 130,
];

/// Repo-free bytes for an image path in `mock_files()`.
pub fn mock_image_bytes(path: &Path) -> Option<&'static [u8]> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("png") => Some(LOGO_PNG),
        _ => None,
    }
}

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
        FileEntry {
            path: PathBuf::from("assets/logo.png"),
            staged: Change::None,
            worktree: Change::Untracked,
            binary: true,
        },
    ]
}

/// Left pane 3: local branches. `is_head` marks the checked-out one.
pub fn mock_branches() -> Vec<BranchEntry> {
    vec![
        BranchEntry {
            name: "main".to_string(),
            is_head: true,
            upstream: Some("origin/main".to_string()),
            ahead: 2,
            behind: 0,
        },
        BranchEntry {
            name: "feat/tui-skeleton".to_string(),
            is_head: false,
            upstream: None,
            ahead: 0,
            behind: 0,
        },
        BranchEntry {
            name: "fix/parse-args".to_string(),
            is_head: false,
            upstream: None,
            ahead: 0,
            behind: 0,
        },
    ]
}

/// Left pane 4: recent commits, newest first.
pub fn mock_commits() -> Vec<CommitEntry> {
    let rows = [
        ("5e04050", "docs: expand the layout plan"),
        ("23023d9", "docs: add inspiration notes"),
        ("2f9bd4f", "docs: drop the arch stub"),
        ("d5bc03c", "chore: initial commit"),
    ];
    rows.iter()
        .enumerate()
        .map(|(i, (hash, summary))| CommitEntry {
            // Pad the 7-char sample to a plausible 40-hex id.
            full_hash: format!("{hash}{}", "0".repeat(33)),
            short_hash: hash.to_string(),
            author: "Max Wells".to_string(),
            summary: summary.to_string(),
            time: 1_725_000_000 - (i as i64 * 3600),
        })
        .collect()
}

/// Left pane 5: stash entries. Empty in the canonical state.
pub fn mock_stashes() -> Vec<StashEntry> {
    Vec::new()
}

/// Bottom box: the commands a real run would have shelled out.
pub const COMMAND_LOG: &[&str] = &["$ git status --porcelain", "$ git diff src/main.rs"];

/// Bottom line: inert lazygit-style key hints, `Label: key | ...`. None of
/// these do anything yet except `?` and `q`.
pub const KEYBAR: &str = "Stage: <space> | Commit: c | Push: P | Pull: p | Scroll diff: J/K | Hunk: ]/[ | Keybindings: ? | Quit: q";

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
pub const HELP: &str = "1 .. 5            focus that pane
Tab / Right       focus next pane
Shift-Tab / Left  focus previous pane
j / Down          move selection down
k / Up            move selection up
J / K             scroll the diff pane
PgUp / PgDn       scroll the diff pane a page
Ctrl-u / Ctrl-d   scroll the diff pane a half page
< / >             diff pane to top / bottom
] / [             next / previous hunk or file
mouse wheel       scroll the pane under the pointer
?                 toggle this help
q / Ctrl-c        quit";
