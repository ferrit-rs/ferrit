# Plan: phase 6, commit

## Goal

Commit the staged index from phase 5. A message-input popup, then `git
commit`, plus the three shapes lazygit and gitu expose: **amend** the last
commit, **reword** the last commit's message, and stage a **`fixup!` /
`squash!`** commit for a later autosquash rebase. This phase moves `HEAD`.
Rebasing to actually apply the fixups is phase 10; phase 6 only creates them.

Scope is `PLAN_0_GENERAL.md`'s phase 6 line: "commit popup (message input),
amend, fixup". A full commit-message editor with body wrapping rules, a
conventional-commit assistant, and AI-generated messages are out (the last one
is explicitly "Not scheduled" in `PLAN_0`).

## Approach: shell out to `git commit`, do not use `git2`

lazygit runs `git commit` (`pkg/commands/git_commands/commit.go`,
`CommitCmdObj`); gitu does the same (`tui/gitu/src/git/mod.rs`). ferrit
follows, for the same reasons phase 3 shelled out for diffs:

- **Hooks run.** `pre-commit`, `prepare-commit-msg`, `commit-msg`,
  `post-commit`. `git2::Repository::commit` runs none of them. A git TUI that
  silently skips `pre-commit` is a bug.
- **GPG / SSH signing works.** `commit.gpgSign`, `gpg.format`, `user.signingKey`
  are honoured because it is the user's `git`. Reimplementing signing over
  `git2` is out of the question.
- **`commit.template`, `core.editor` conventions, `commit.verbose`** all apply.
  ferrit supplies the message with `-m` / `-F` so it does not open
  `core.editor`, but the rest of the user's commit config still takes effect.
- **Sign-off is a flag, not string surgery.** `git commit -s` appends a
  correct `Signed-off-by:` trailer using `user.name` / `user.email`. `AGENTS.md`
  and the repo's DCO note make `-s` worth a toggle.

`--amend`, `--fixup=<hash>`, `--squash=<hash>`, `--no-verify`, `--signoff` are
all plain flags on the same subprocess.

## Popup UX

A modal over the normal screen. The left panes and diff dim; keystrokes go to
the popup until it closes. This is the first real input popup in ferrit
(`PLAN_1_LAYOUT.md` listed "Input popups" as out of scope for phase 1, meaning
"later"; this is later).

```
┌ [1] Status ───────────┐┌ Staged changes ──────────────────────────────────┐
│ ferrit → main ↑2      ││  diff --git a/src/app.rs b/src/app.rs            .│
└───────────────────────┘│  @@ -40,6 +40,8 @@                               .│
┌ [2] Files ────────────┐│  +    self.mode = Mode::Diff;                    .│
│  M  src/app.rs        .││                                                 .│
│                       ┌ Commit ─────────────────────────────────────────┐ .│
│                       │ feat(stage): line-level staging in the diff     │ .│
│                       │                                                 │ .│
│                       │ Stage/unstage individual lines with V-select,   │ .│
│                       │ piped through git apply --cached --recount.     │ .│
│                       │█                                                │ .│
│                       │                                                 │ .│
│                       ├─────────────────────────────────────────────────┤ .│
│                       │ 2 files staged   sign-off: on   verify: on      │ .│
│                       │ ^S commit   ^O sign-off   ^N no-verify   Esc    │ .│
│                       └─────────────────────────────────────────────────┘ .│
└───────────────────────┘└──────────────────────────────────────────────────┘
 Commit: c | Amend: A | Reword: w | Fixup: f | Keybindings: ? | Quit: q
```

- Subject line and body in one `tui-textarea` (multiline). First line is the
  subject; a blank second line is inserted on save if the user typed a body
  straight after the subject, matching git convention. No hard wrap, no
  enforced 50/72 (a lint hint in the footer is optional polish, not phase 6).
- Footer shows the precondition (`N files staged`) and the two toggles.
- `Ctrl-S` (or `Ctrl-Enter`) commits. `Esc` cancels and **keeps the draft**
  (see "Draft persistence").

### Amend / reword / fixup entry points

| Key (Nav mode) | Popup opens with | Subprocess on commit |
| --- | --- | --- |
| `c` | empty message (or `commit.template` contents) | `git commit -F -` |
| `A` | last commit's message pre-filled, title `Amend HEAD` | `git commit --amend -F -` |
| `w` | last commit's message pre-filled, title `Reword HEAD`, diff pane hidden | `git commit --amend -F - --only` (no staged changes needed) |
| `f` | **no** text box, a commit picker instead (see below) | `git commit --fixup=<hash>` |
| `s` on a commit row | as `f` but `--squash=<hash>`, opens the text box for the squash message | `git commit --squash=<hash> -F -` |

Reword uses `--amend --only` so it does not fold in whatever happens to be
staged; lazygit's reword does the same. Amend (`A`) *does* include the staged
index, that is the point of it.

### The fixup picker (`f`)

`fixup!` targets a commit. Rather than a free-text hash, `f` focuses the
Commits pane in a "pick a target" state: `j` / `k` to choose, `Enter` to
create `git commit --fixup=<full_hash>`, `Esc` to cancel. No message box: git
generates `fixup! <subject>` itself. This reuses the phase 1..2 Commits list
and its selection cursor; it is a mode flag, not a new widget.

```
┌ [4] Commits - Reflog  ┐   f pressed: "pick the commit to fix up"
│ 5e04050 MW o feat: …  │   Enter -> git commit --fixup=5e04050c9a1b...
│>23023d9 MW o fix: …   │<--pick cursor (yellow border, distinct from blue)
│ 2f9bd4f MW o docs: …  │   Esc  -> back to Nav, no commit
└───────────────1 of 3 ─┘
```

## Backend: `src/git/commit.rs`

New module under `src/git/`, sibling of `apply.rs` (phase 5). No `ratatui`.
Reuses the `git -C <workdir> ...` spawn pattern from `diff.rs` / `apply.rs`.

```rust
//! Create commits by shelling out to `git commit`, so hooks, signing and
//! commit.* config all apply. See docs/PLAN_6_COMMIT.md.

/// What kind of commit to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitKind {
    Normal,
    Amend,
    /// Reword: amend the message only, ignore the staged index.
    Reword,
    Fixup  { target: String },   // full hash
    Squash { target: String },   // full hash
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitOpts {
    pub sign_off: bool,     // -s   (default: read from config / DCO, see below)
    pub no_verify: bool,    // -n   (default false)
}

impl Repo {
    /// Run `git commit` with the message on stdin (`-F -`). `message` is
    /// ignored for `Fixup` (git writes `fixup! <subject>` itself). Returns the
    /// new commit's full hash on success.
    pub fn commit(&self, kind: &CommitKind, message: &str, opts: CommitOpts)
        -> GitResult<String>;

    /// HEAD's message, for pre-filling the Amend / Reword box. `None` on an
    /// unborn branch.
    pub fn head_message(&self) -> GitResult<Option<String>>;

    /// Count of staged paths (`git diff --cached --name-only`), the popup's
    /// precondition. 0 disables `c` (but not `A` amend of a message, nor `w`).
    pub fn staged_count(&self) -> GitResult<usize>;
}
```

`GitError` gains, matching phase 3/5 shape:

```rust
#[error("git commit failed: {0}")]
CommitFailed(String),

#[error("nothing staged to commit")]
NothingStaged,

#[error("commit hook rejected the commit:\n{0}")]
HookRejected(String),
```

`git commit` distinguishes these by exit + stderr:

```
  "nothing to commit"                       -> NothingStaged
  hook non-zero (pre-commit / commit-msg)   -> HookRejected(stderr)  (verbatim,
                                               it is the hook's own message)
  anything else non-zero                    -> CommitFailed(stderr)
  exit 0                                    -> parse the new HEAD, return hash
```

`HookRejected` is shown in full in a dismissible note (hooks print actionable
output: which file failed lint, etc.), not truncated into one Status line.

### Message on stdin, never a temp file, never the editor

`git commit -F -` reads the message from stdin. No `core.editor` spawn (ferrit
*is* the editor here), no tempfile to clean up. The `commit-msg` hook can still
rewrite it; ferrit re-reads `HEAD` after and shows the result.

### Sign-off default

`AGENTS.md`: normal work pushes to `main`, external contributions come in under
DCO with `git commit -s`. Default the `sign_off` toggle from, in order:
`commit.gpgSign`-style config if a `ferrit.signOff` key ever exists (phase 11),
else a `--signoff`-implying env, else **off**, with `Ctrl-O` to flip it per
commit. Do not silently sign every commit; make it visible in the footer.

## App wiring

Phase 5 added `mode: Mode`. Phase 6 adds a `Popup` that, when `Some`, owns all
input (like `show_help` does today in `on_key`, but richer).

```rust
enum Popup {
    Commit(CommitDraft),      // c / A / w / s
    FixupPick,                // f: Commits pane is the picker
    Note(String),             // HookRejected / CommitFailed, dismissible
    Confirm { .. },           // reused from phase 5's discard confirm
}

struct CommitDraft {
    textarea: tui_textarea::TextArea<'static>,
    kind: CommitKind,
    sign_off: bool,
    no_verify: bool,
}

struct App {
    // ...phase 3..5 fields...
    popup: Option<Popup>,
    /// Survives an Esc-cancel of the commit popup so a mistyped keystroke does
    /// not lose a paragraph. Cleared on a successful commit.
    commit_draft: Option<String>,
}
```

`on_key` gains an early branch, same structure as the existing `show_help`
check:

```
on_key(key):
    if let Some(popup) = &mut self.popup {
        return self.popup_key(key)        // popup swallows everything
    }
    ... existing Nav / Diff handling ...
    'c' => self.open_commit(CommitKind::Normal)
    'A' => self.open_commit(CommitKind::Amend)
    'w' => self.open_commit(CommitKind::Reword)
    'f' => self.popup = Some(Popup::FixupPick)
```

`popup_key` for `Popup::Commit`: printable keys and editing keys go to
`textarea.input(key)`; `Ctrl-S` -> `do_commit()`; `Ctrl-O` / `Ctrl-N` flip the
toggles; `Esc` -> stash `textarea` text into `commit_draft`, close.

### `do_commit()`

```
do_commit():
    msg = draft.textarea.lines().join("\n").trim_end()
    if kind == Normal && msg subject line empty -> keybar note "empty message", stay
    match repo.commit(&kind, &msg, opts):
        Ok(hash) ->
            self.commit_draft = None
            self.popup = None
            self.refresh()                 # HEAD moved: Commits, Status, Files all change
            self.update_right_pane()       # staged diff is now empty -> Note
            keybar note "committed <short_hash>"
        Err(HookRejected(out)) -> self.popup = Some(Popup::Note(out))   # draft kept
        Err(NothingStaged)     -> self.popup = Some(Popup::Note("nothing staged"))
        Err(CommitFailed(e))   -> self.popup = Some(Popup::Note(e))     # draft kept
```

After `HEAD` moves, phase 2's `refresh()` already re-reads the Commits pane, so
the new commit appears at the top with no extra code. Phase 3's
`update_right_pane` sees an empty staged diff and renders the "no changes"
`Note`. Phase 5's diff cursor, if it was in `Mode::Diff`, drops to `Nav`
because the staged side is now empty.

A commit made from another shell arrives as an `AppEvent::Refresh` (phase 2.5
fs-watch on `.git/`), so the Commits pane updates on its own, same as staging
did in phase 5.

## Rendering (`src/ui/`)

- New `src/ui/popup.rs`: a centered `Clear` + bordered `Block`, sized to a
  fraction of the frame (min 40 wide, grows with the terminal), with the
  `tui-textarea` widget inside and a two-line footer. The `Clear` widget wipes
  what is under it; the rest of the screen is drawn first and dimmed via a
  `Style` pass (or just left as-is if dimming complicates the buffer).
- `Popup::Note`: same box, a `Paragraph { wrap }` of the hook output, `Esc` or
  `Enter` to dismiss, a scroll if it overflows (`Ctrl-d`/`u` reused).
- `Popup::FixupPick`: no box. The Commits pane border turns yellow (a new
  `theme` role, distinct from the green focus border and the blue selection
  bar) and the keybar switches to `Enter pick   Esc cancel`.
- Keybar (`mock::KEYBAR`) gains `Commit: c | Amend: A | Reword: w | Fixup: f`.
  `HELP` gains the same. The bar is context-sensitive from phase 5 already;
  phase 6 adds the popup-open variant.

## Keybindings (new in phase 6)

| Key | Context | Action |
| --- | --- | --- |
| `c` | Nav, index has staged changes | open the commit popup |
| `A` | Nav, at least one commit on the branch | open amend popup, message pre-filled |
| `w` | Nav, at least one commit | open reword popup (message only, `--only`) |
| `f` | Nav | enter fixup-pick on the Commits pane |
| `s` | Nav, Commits focused | open squash popup targeting the selected commit |
| `Ctrl-S` / `Ctrl-Enter` | commit popup | create the commit |
| `Ctrl-O` | commit popup | toggle sign-off (`-s`) |
| `Ctrl-N` | commit popup | toggle no-verify (`-n`) |
| `Esc` | any popup | cancel (commit popup keeps the draft) |
| `Enter` | fixup-pick | create `git commit --fixup=<hash>` |

`c` is disabled (inert, brief keybar note "nothing staged") when
`staged_count() == 0`. `A` and `w` are enabled even with an empty index
because amending a message is valid; `A` with nothing staged just reuses the
tree, which is what `git commit --amend` does.

## Edge cases

| Case | Behaviour |
| --- | --- |
| unborn branch (fresh repo, no `HEAD`) | `c` works (root commit); `A` / `w` / `f` disabled with a keybar note |
| empty subject line | commit blocked, popup stays, keybar note; a body with no subject is not allowed |
| `pre-commit` hook rewrites files | git aborts with "files were modified by this hook"; shown as `HookRejected`, draft kept, `refresh()` so the user sees the new worktree state |
| `commit-msg` hook rewrites the message | commit succeeds; ferrit re-reads `HEAD` and the Commits pane shows the final message |
| GPG passphrase prompt | `git` needs a tty or a pinentry; if it blocks, the subprocess hangs. Phase 6 runs `git commit` with inherited stdio for exactly this reason, or documents that a gpg-agent / pinentry-tty must be configured. Revisit if it bites. |
| detached HEAD | `c` commits onto the detached HEAD (git allows it, with its own warning in stderr, surfaced as a `Note`) |
| merge in progress (`MERGE_HEAD` present) | `c` finalizes the merge commit; message pre-filled from `MERGE_MSG`. Full merge-conflict flow is phase 10; a clean merge commit is fine here. |
| amend a pushed commit | ferrit does not warn (phase 8 tracks ahead/behind; a "this is published" guard is a phase 8+ polish) |
| `--no-verify` on | footer shows `verify: off` in red so it is never a silent skip |
| commit succeeds, fs-watch also fires | `do_commit`'s explicit `refresh()` and the `AppEvent::Refresh` both run `refresh()`; it is idempotent, no double commit |

## Self-testing (see `PLAN_SELF_TESTING.md`)

Throwaway `git2` fixture repos, `git` run against them; replay scripts wait on
ST1..ST3 like phases 3 and 5.

- `tests/git_commit.rs`: fixture repo, then via `Repo`:
  - `commit(Normal)` with a staged file: `git log -1 --format=%s` matches;
    `git rev-parse HEAD^` is the old head (parent is correct).
  - `commit(Normal)` with nothing staged -> `NothingStaged`.
  - `commit(Amend)` changes the subject and keeps the parent
    (`git rev-parse HEAD^` unchanged).
  - `commit(Reword)` with a *dirty* index: the staged file is **not** in the
    amended commit (`--only`), only the message changed.
  - `commit(Fixup { target })`: `git log -1 --format=%s` is
    `fixup! <target subject>`; no message passed.
  - a repo with a `pre-commit` hook that `exit 1`s -> `HookRejected`, and
    `git rev-parse HEAD` is unchanged (no commit made).
  - `sign_off: true` -> the message has a `Signed-off-by:` trailer with the
    fixture's `user.email`.
  - root commit on an unborn branch.
- `tests/app_commit.rs` (extends phase 5's `app_stage.rs`): stage a file,
  press `c`, type a message into the draft, `Ctrl-S`, assert `app.commits[0]`
  is the new commit, `app.popup` is `None`, `app.commit_draft` is `None`, the
  staged diff view is a `Note`. Then press `c` again, type, `Esc`, press `c`
  once more and assert the draft came back.
- `tests/render.rs`: `TestBackend` snapshot of the commit popup (box, textarea
  with two lines of text, footer with toggles) and of the fixup-pick yellow
  border on the Commits pane.
- `test/scripts/50-commit.script` + inline git golden:

  ```
  size 120x40
  fixture canonical
  key 2                       # Files
  key a                       # stage all (phase 5)
  key c                       # commit popup
  type "test: replay commit"
  key ctrl-s
  snapshot after-commit
  git log -1 --format=%s          -> "test: replay commit"
  git status --porcelain=v2       -> ""
  git rev-parse --abbrev-ref HEAD -> "main"
  ```

## Dependencies

```toml
tui-textarea = { version = "0.7", default-features = false, features = ["crossterm", "ratatui"] }
```

`tui-textarea` is the multiline editing widget `INSPIRATION.md` already names
for "commit messages, interactive rebase todo editing". Check crates.io for the
version that matches the `ratatui 0.30` in the tree; pin it. It is the only
add: `git commit` is a subprocess, no new git library surface.

## Milestones

- **C0** `src/git/commit.rs`: `CommitKind`, `CommitOpts`, `Repo::commit`
  (Normal + Amend), `head_message`, `staged_count`. `GitError::CommitFailed`
  / `NothingStaged` / `HookRejected`. `tests/git_commit.rs` Normal + Amend +
  NothingStaged + hook cases green.
- **C1** `tui-textarea` dep. `src/ui/popup.rs`, `Popup::Commit`,
  `CommitDraft`. `c` opens it, `Ctrl-S` commits, `Esc` cancels with draft
  kept. `on_key` popup branch. `refresh()` + `update_right_pane` after a
  successful commit. `tests/app_commit.rs` green.
- **C2** `A` amend (message pre-filled from `head_message`), `w` reword
  (`--amend --only`, diff pane hidden in the popup). Unborn-branch disables.
- **C3** `f` fixup-pick on the Commits pane (yellow border theme role,
  `Enter` / `Esc`), `git commit --fixup=<hash>`. `s` squash from a Commits
  row with a message box.
- **C4** `Ctrl-O` sign-off toggle (`-s`, footer state), `Ctrl-N` no-verify
  toggle (`-n`, red footer state). `Popup::Note` for `HookRejected` /
  `CommitFailed`, full text, dismissible, scrollable. Keybar + `HELP` +
  `mock::KEYBAR` updated.
- **C5** polish: `cargo clippy --all-targets` clean, no warnings; every edge
  case in the table has a test or an explicit inert path; a commit made from
  another shell mid-popup does not corrupt state; `tests/render.rs` popup
  snapshots; `50-commit.script` ready for the harness.

## Definition of done (phase 6)

- `c` commits the staged index with a typed message; the Commits pane shows
  the new commit at the top and the staged diff empties, with no keypress
  beyond the commit.
- `A` amends `HEAD` (message + staged tree), `w` rewords `HEAD` (message
  only), each pre-filled with the current message.
- `f` creates a `fixup!` commit against a commit picked from the Commits
  pane; `s` creates a `squash!` with its own message.
- All commit hooks and signing config run because it is the user's `git
  commit`; a hook rejection shows the hook's full output and makes no commit.
- Sign-off and no-verify are per-commit toggles, both visible in the popup
  footer; no-verify reads as a warning.
- Cancelling the commit popup with `Esc` keeps the draft for the next `c`.
- `src/git/` still has no `ratatui` import (`cargo tree` check from phase 2).
- `cargo clippy --all-targets` clean; `tests/git_commit.rs`,
  `tests/app_commit.rs` pass; the unborn-branch and merge-in-progress paths
  do not panic.

## After phase 6

Phase 7 is branches: checkout, create, delete, fast-forward, merge. It uses
the Commits pane picker pattern this phase introduced for `f`, and the popup
primitive from `src/ui/popup.rs` for the "new branch name" input.

Deferred out of phase 6, their own phases or a follow-up:

- applying the `fixup!` / `squash!` commits via autosquash rebase (`git rebase
  -i --autosquash`), phase 10.
- a "this commit is already pushed, amend anyway?" guard, phase 8 (needs
  upstream tracking).
- 50/72 subject/body lint, `commit.template` rendering, a conventional-commit
  scaffold: phase 11 polish.
- AI-generated commit messages (`INSPIRATION.md`: lazygitrs, gmsg): "Not
  scheduled" in `PLAN_0_GENERAL.md`, revisit after phase 11.
- opening `$EDITOR` for the message instead of the in-TUI textarea: a config
  option, phase 11.
