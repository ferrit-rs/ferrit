//! Diff-cursor navigation and hunk/line staging (`Mode::Diff`).

use super::{
    App, ApplyDir, ApplyTarget, ConfirmAction, ConfirmPrompt, DiffCursor, DiffSide, DiffView,
    FileRow, GitResult, Granule, KeyCode, KeyEvent, Mode, Pane, Range, events, git,
    hunk_content_id, hunk_id_at, hunk_lines_for, selectable_lines,
};

impl App {
    /// The `FileEntry` behind the Files pane's current selection, or `None`
    /// on a directory row or an empty pane.
    pub(super) fn selected_file(&self) -> Option<&git::model::FileEntry> {
        let rows = self.files_tree_rows();
        let FileRow::File { index, .. } = rows.get(self.selected(Pane::Files))? else {
            return None;
        };
        self.files.get(*index)
    }

    /// The `Diff` the cursor currently lives in (`Mode::Diff`'s active
    /// side), or `None` off the Files pane / without a real Files split.
    pub(super) fn cursor_diff(&self) -> Option<&git::diff::Diff> {
        let DiffView::Files(files) = &self.diff else {
            return None;
        };
        Some(match self.cursor.side {
            DiffSide::Worktree => &files.unstaged,
            DiffSide::Staged => &files.staged,
        })
    }

    /// Scroll the shared Files-split viewport so `cursor.line` stays on
    /// screen, the same "a jump always lands visibly" rule phase 3's `]` /
    /// `[` already follows.
    pub(super) fn ensure_cursor_visible(&mut self) {
        let viewport = self.right_viewport.max(1);
        if self.cursor.line < self.right_scroll {
            self.right_scroll = self.cursor.line;
        } else if self.cursor.line >= self.right_scroll + viewport {
            self.right_scroll = self.cursor.line + 1 - viewport;
        }
        self.clamp_right_scroll();
    }

    /// `Enter` / `l` on a Files-pane file row (`Mode::Nav`): focus the diff
    /// for staging within it. A no-op off the Files pane, on a directory
    /// row, already in `Mode::Diff`, or when neither side has a selectable
    /// line to put the cursor on (binary, a pure rename, no change at all) —
    /// those stage whole-file only, from `Mode::Nav`.
    pub(super) fn enter_diff_mode(&mut self) {
        if self.focus != Pane::Files || self.mode == Mode::Diff {
            return;
        }
        let Some(entry) = self.selected_file() else {
            return;
        };
        let DiffView::Files(files) = &self.diff else {
            return;
        };
        // Same direction rule as the file-level toggle: worktree changes
        // lead, so a half-staged file's cursor starts where there's still
        // something to stage.
        let side = if entry.worktree == git::model::Change::None {
            DiffSide::Staged
        } else {
            DiffSide::Worktree
        };
        let diff = match side {
            DiffSide::Worktree => &files.unstaged,
            DiffSide::Staged => &files.staged,
        };
        let hunks = hunk_lines_for(diff);
        let Some(hunk) = hunks.iter().find(|hl| !hl.selectable.is_empty()) else {
            return;
        };
        let Some(&line) = hunk.selectable.first() else {
            return;
        };

        self.mode = Mode::Diff;
        self.cursor = DiffCursor {
            side,
            line,
            anchor: None,
            hunk_id: hunk_content_id(diff, hunk.hunk_index),
        };
        self.ensure_cursor_visible();
    }

    /// `Esc` / `h` in `Mode::Diff`: back to `Mode::Nav`.
    pub(super) fn leave_diff_mode(&mut self) {
        self.mode = Mode::Nav;
    }

    /// `j` / `k` in `Mode::Diff`: move the line cursor over selectable
    /// lines only, `docs/PLAN_6_STAGING.md`'s "context lines are
    /// unselectable".
    pub(super) fn move_diff_cursor(&mut self, dir: isize) {
        let Some(diff) = self.cursor_diff() else {
            return;
        };
        let lines = selectable_lines(diff);
        let Some(pos) = lines.iter().position(|&l| l == self.cursor.line) else {
            return;
        };
        let next = if dir > 0 {
            pos.saturating_add(1).min(lines.len().saturating_sub(1))
        } else {
            pos.saturating_sub(1)
        };
        let Some(&line) = lines.get(next) else {
            return;
        };
        // Crossing into a different hunk: re-tag `hunk_id` right away, or
        // the very next keystroke's `resync_diff_cursor` (which runs on
        // every key, not just a stage) reads the stale id, decides the old
        // hunk "lost" this line, and snaps the cursor straight back to it.
        let hunk_id = hunk_id_at(diff, line);
        self.cursor.line = line;
        if let Some(id) = hunk_id {
            self.cursor.hunk_id = id;
        }
        self.ensure_cursor_visible();
    }

    /// `V` in `Mode::Diff`: start or clear a line V-selection.
    pub(super) fn toggle_diff_anchor(&mut self) {
        self.cursor.anchor = if self.cursor.anchor.is_some() {
            None
        } else {
            Some(self.cursor.line)
        };
    }

    /// `]` / `[` in `Mode::Diff`: move the cursor to the next / previous
    /// hunk's first selectable line. Unlike the `Mode::Nav` `]` / `[`
    /// (`jump_diff_anchor`), which scrolls a single commit diff and is a
    /// no-op on the Files split, this moves the cursor itself.
    pub(super) fn jump_diff_cursor_hunk(&mut self, dir: isize) {
        let Some(diff) = self.cursor_diff() else {
            return;
        };
        let starts: Vec<usize> = hunk_lines_for(diff)
            .iter()
            .filter_map(|hl| hl.selectable.first().copied())
            .collect();
        let cur = self.cursor.line;
        let target = if dir > 0 {
            starts.iter().find(|&&l| l > cur).copied()
        } else {
            starts.iter().rev().find(|&&l| l < cur).copied()
        };
        if let Some(line) = target {
            let hunk_id = hunk_id_at(diff, line);
            self.cursor.line = line;
            if let Some(id) = hunk_id {
                self.cursor.hunk_id = id;
            }
            self.ensure_cursor_visible();
        }
    }

    /// What `<space>` / `d` would act on right now: the V-selected lines
    /// when there is a selection, else the whole hunk under the cursor
    /// (`docs/PLAN_6_STAGING.md` "Granule resolution", S1's simplified
    /// rule). `None` when the cursor's hunk cannot be found (should not
    /// happen while `Mode::Diff` is up) or a V-selection covers no `+`/`-`
    /// line (only context was under it — a no-op, not an empty patch).
    pub(super) fn current_granule(&self) -> Option<Granule> {
        let diff = self.cursor_diff()?;
        let file = diff.files.first()?;
        let hunks = hunk_lines_for(diff);
        let hl = hunks
            .iter()
            .find(|hl| hl.lines.contains(&self.cursor.line))?;
        let hunk = file.hunks.get(hl.hunk_index)?;

        if let Some(anchor) = self.cursor.anchor {
            let lo = anchor.min(self.cursor.line);
            let hi = anchor.max(self.cursor.line);
            let lines: Vec<usize> = hl
                .selectable
                .iter()
                .filter(|&&l| (lo..=hi).contains(&l))
                .map(|&l| l - hl.lines.start)
                .collect();
            if lines.is_empty() {
                return None;
            }
            Some(Granule::Lines {
                file_header: diff.text.get(file.header.clone())?.to_owned(),
                hunk_header: diff.text.get(hunk.header.clone())?.to_owned(),
                hunk_body: diff.text.get(hunk.body.clone())?.to_owned(),
                lines,
            })
        } else {
            let patch = diff.text.get(file.header.start..hunk.body.end)?.to_owned();
            Some(Granule::Hunk { patch })
        }
    }

    /// Run a `Granule` through the matching backend call.
    pub(super) fn apply_granule(
        &self,
        granule: &Granule,
        dir: ApplyDir,
        target: ApplyTarget,
    ) -> GitResult<()> {
        let Some(repo) = &self.repo else {
            return Ok(());
        };
        match granule {
            Granule::Hunk { patch } => repo.apply_hunk(patch, dir, target),
            Granule::Lines {
                file_header,
                hunk_header,
                hunk_body,
                lines,
            } => repo.apply_lines(file_header, hunk_header, hunk_body, lines, dir, target),
        }
    }

    /// Refresh after a stage / unstage / discard, then surface a failure in
    /// the Status pane. `git apply` is atomic per invocation, so a failure
    /// leaves the repository exactly as it was; the refresh still runs so a
    /// failed attempt (context drift from an external edit) re-reads the
    /// current diff for the retry (`docs/PLAN_6_STAGING.md` "apply fails").
    pub(super) fn finish_apply(&mut self, result: GitResult<()>) {
        self.request_refresh();
        if let Err(e) = result {
            self.report_error(e);
        }
    }

    /// `<space>` on a Files row (`Mode::Nav`): stage or unstage the whole
    /// file, direction inferred from which side has a change
    /// (`docs/PLAN_6_STAGING.md` "Stage vs unstage is one key").
    pub(super) fn stage_selected_file(&mut self) {
        if self.focus != Pane::Files {
            return;
        }
        let Some(entry) = self.selected_file() else {
            return;
        };
        let dir = if entry.worktree != git::model::Change::None {
            ApplyDir::Forward
        } else if entry.staged != git::model::Change::None {
            ApplyDir::Reverse
        } else {
            return;
        };
        let path = entry.path.clone();
        let Some(repo) = &self.repo else {
            return;
        };
        let result = repo.stage_file(&path, dir);
        self.finish_apply(result);
    }

    /// `<space>` in `Mode::Diff`: stage/unstage the hunk under the cursor,
    /// or the V-selection when one is active.
    pub(super) fn stage_diff_cursor(&mut self) {
        let Some(granule) = self.current_granule() else {
            return;
        };
        let dir = match self.cursor.side {
            DiffSide::Worktree => ApplyDir::Forward,
            DiffSide::Staged => ApplyDir::Reverse,
        };
        let result = self.apply_granule(&granule, dir, ApplyTarget::Index);
        self.cursor.anchor = None;
        self.finish_apply(result);
    }

    /// `a` (Nav, Files focused): stage every changed file if any is
    /// unstaged, else unstage everything — one `git` call either way
    /// (`docs/PLAN_6_STAGING.md` milestone S4).
    pub(super) fn stage_all_files(&mut self) {
        if self.focus != Pane::Files {
            return;
        }
        let dir = if self
            .files
            .iter()
            .any(|f| f.worktree != git::model::Change::None)
        {
            ApplyDir::Forward
        } else if self
            .files
            .iter()
            .any(|f| f.staged != git::model::Change::None)
        {
            ApplyDir::Reverse
        } else {
            return;
        };
        let Some(repo) = &self.repo else {
            return;
        };
        let result = repo.stage_all(dir);
        self.finish_apply(result);
    }

    /// `d`: ask before discarding a worktree change, at the file granularity
    /// from `Mode::Nav` (Files focused) or at the hunk / line granularity
    /// under the cursor from `Mode::Diff`. Discard only ever touches the
    /// worktree (`docs/PLAN_6_STAGING.md`'s own scope), so it is a no-op on
    /// the Staged side and on a file with no worktree change of its own.
    pub(super) fn discard_prompt(&mut self) {
        match self.mode {
            Mode::Nav if self.focus == Pane::Files => {
                let Some(entry) = self.selected_file() else {
                    return;
                };
                if entry.worktree == git::model::Change::None {
                    return;
                }
                self.pending_confirm = Some(ConfirmPrompt {
                    message: format!("discard all changes in {}?", entry.path.display()),
                    action: ConfirmAction::DiscardFile(entry.path.clone()),
                });
            },
            Mode::Diff if self.cursor.side == DiffSide::Worktree => {
                let Some(granule) = self.current_granule() else {
                    return;
                };
                let Some(entry) = self.selected_file() else {
                    return;
                };
                let what = match &granule {
                    Granule::Hunk { .. } => "this hunk".to_owned(),
                    Granule::Lines { lines, .. } => {
                        format!(
                            "{} line{}",
                            lines.len(),
                            if lines.len() == 1 { "" } else { "s" }
                        )
                    },
                };
                self.pending_confirm = Some(ConfirmPrompt {
                    message: format!("discard {what} in {}?", entry.path.display()),
                    action: ConfirmAction::DiscardGranule(granule),
                });
            },
            Mode::Nav | Mode::Diff => {},
        }
    }

    /// `y` while a confirm prompt is up: run its action. A branch delete
    /// refused for being unmerged (`"is not fully merged"`, the same
    /// stable-substring technique `commit.rs`'s `NothingStaged` already
    /// uses) re-opens the confirm one more time asking to force it,
    /// rather than reporting the refusal and stopping — `git branch -d`
    /// is offering a choice, not failing outright.
    pub(super) fn run_confirm(&mut self) {
        let Some(prompt) = self.pending_confirm.take() else {
            return;
        };
        match prompt.action {
            ConfirmAction::SelectAuthor(identity) => {
                self.selected_author = identity;
            },
            ConfirmAction::DiscardFile(path) => {
                let untracked = self
                    .files
                    .iter()
                    .find(|f| f.path == path)
                    .is_some_and(|f| f.worktree == git::model::Change::Untracked);
                let result = match &self.repo {
                    Some(repo) => repo.discard_file(&path, untracked),
                    None => return,
                };
                self.cursor.anchor = None;
                self.finish_apply(result);
            },
            ConfirmAction::DiscardGranule(granule) => {
                let result = self.apply_granule(&granule, ApplyDir::Reverse, ApplyTarget::Worktree);
                self.cursor.anchor = None;
                self.finish_apply(result);
            },
            ConfirmAction::DeleteBranch { name, force } => {
                let Some(repo) = &self.repo else { return };
                match repo.delete_branch(&name, force) {
                    Ok(()) => self.request_refresh(),
                    Err(git::error::GitError::BranchFailed(msg))
                        if !force && msg.contains("is not fully merged") =>
                    {
                        self.pending_confirm = Some(ConfirmPrompt {
                            message: format!(
                                "'{name}' is not fully merged. Force delete? This may lose \
                                 commits with no other reference to them."
                            ),
                            action: ConfirmAction::DeleteBranch { name, force: true },
                        });
                    },
                    Err(e) => self.report_error(e),
                }
            },
            ConfirmAction::DropStash { oid } => self.drop_stash(&oid),
            ConfirmAction::ForcePush => {
                if let Some(sender) = self.event_sender.clone() {
                    self.start_remote_op_with_force(
                        events::RemoteOp::Push,
                        None,
                        None,
                        true,
                        sender,
                    );
                }
            },
        }
    }

    /// Cursor state for the right-pane render: `(side, cursor line, V-select
    /// range)` while `Mode::Diff` is up, else `None`. The range is
    /// inclusive-exclusive (`a..b`) over `side`'s own `Diff::text` lines.
    pub fn diff_cursor(&self) -> Option<(DiffSide, usize, Option<Range<usize>>)> {
        if self.mode != Mode::Diff {
            return None;
        }
        let range = self
            .cursor
            .anchor
            .map(|a| a.min(self.cursor.line)..a.max(self.cursor.line) + 1);
        Some((self.cursor.side, self.cursor.line, range))
    }

    /// Right-pane title suffix while `Mode::Diff` is up: `hunk 1/3` or
    /// `lines 41-42`/`line 41`, so it is obvious what `<space>` will hit.
    pub fn diff_granule_hint(&self) -> Option<String> {
        if self.mode != Mode::Diff {
            return None;
        }
        let diff = self.cursor_diff()?;
        let hunks = hunk_lines_for(diff);
        let total = hunks.len();
        let current = hunks
            .iter()
            .position(|hl| hl.lines.contains(&self.cursor.line))?;
        Some(match self.cursor.anchor {
            Some(anchor) => {
                let lo = anchor.min(self.cursor.line) + 1;
                let hi = anchor.max(self.cursor.line) + 1;
                if lo == hi {
                    format!("line {lo}")
                } else {
                    format!("lines {lo}-{hi}")
                }
            },
            None => format!("hunk {}/{total}", current + 1),
        })
    }

    /// Keys meaningful only in `Mode::Diff`. Returns whether `key` was one
    /// of them, so `on_key` falls through to the ordinary right-pane scroll
    /// keys (`J`/`K`/`PageUp`/`PageDown`/`Ctrl-d`/`u`/`<`/`>`) otherwise —
    /// those still just scroll the shared viewport, unchanged from phase 3.
    pub(super) fn on_diff_key(&mut self, key: KeyEvent) -> bool {
        if self.mode != Mode::Diff {
            return false;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => self.leave_diff_mode(),
            KeyCode::Char('j') | KeyCode::Down => self.move_diff_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_diff_cursor(-1),
            KeyCode::Char(']') => self.jump_diff_cursor_hunk(1),
            KeyCode::Char('[') => self.jump_diff_cursor_hunk(-1),
            KeyCode::Char('V') => self.toggle_diff_anchor(),
            KeyCode::Char(' ') => self.stage_diff_cursor(),
            KeyCode::Char('d') => self.discard_prompt(),
            _ => return false,
        }
        true
    }
}
