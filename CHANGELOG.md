# Changelog

All notable changes to ferrit are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Branches are listed with the checked-out one first, then the most recently committed to, as in lazygit; before, alphabetically.

- The new-branch prompt names the branch it starts from: `New branch name (branch is off of 'main')`, as in lazygit.

- `r` rewords the selected commit in Commits, as in lazygit; `w` still works and the key bar now shows `Reword: r`.

- A commit's Patch has the `---` line and the per-file stat (`a.txt | 1 +`, `1 file changed, 1 insertion(+)`) between its message and the diff, as in lazygit; before it went straight from the message to the diff.
- Commit names show where they point: `(HEAD -> main, tag: v0.6.0, origin/main)` on a commit's Patch and on the branch Log, and a commit's tags in the Commits list before its subject. Before, no tag or ref was shown anywhere.
- In the Commits list the hash is red for a commit not pushed yet, yellow for one pushed, and green for one merged into `origin/main` (or `origin/master`), as in lazygit; before every hash was green.

- After a command that prints something on stdout (a commit: `[main 3cd9f42] docs: flow-compare demo`), the Infos box shows that first line under the command, as lazygit's command log does with git's output. Before, only `$ git commit -F -` showed, with no sign of what git answered.

- `Space` on a directory row in Files stages every change under it, or unstages them all when none is left to stage, as in lazygit; before it did nothing on a directory. On the root row it is every file. Conflicted files that still hold markers block it, with the same message as for a single file.

- In the Files pane, the staged letter is green and the unstaged one red, so staging a file changes its colour, and an untracked file shows as `??` in red, as in lazygit; before, a staged `M` and an unstaged `M` were both yellow and an untracked file was a white `?`.

- With nothing to commit, the Files pane's right side is titled `Diff` and says `No changed files`, as in lazygit; before it kept the title `Unstaged changes` over an empty box.

- Inside a commit's files (Enter on a commit) or a branch's log (Enter on a branch), the key hint bar shows `Back: esc | Open: enter`; before, it kept the Files keys (`Stage`, `Commit`, `Amend`, `Reword`), none of which do anything on those rows.

- After creating a branch (`n`) the selection in Branches moves to it, and after a commit the selection in Commits moves to the new commit at the top, as in lazygit; before, each stayed on the row it was on, so the new branch could end up off screen and the right pane kept showing an older commit.

- The mouse wheel over a left pane scrolls that pane's list, whichever pane has the focus; before, it moved the focused pane's selection. The list scrolls two rows a tick and the selection stays where it was, even off screen, with the right pane still showing it, as in lazygit; any key or click that changes the selection brings the view back to it. Over the right pane the wheel scrolls the diff as before, and elsewhere it does nothing.

## [0.6.0] - 2026-09-25

- With `[ui] mouse = false`, ferrit ignores mouse events even if the terminal still sends them; before, only mouse capture was left off.
- The commit editor's Summary shows its length as `n/50` on its bottom border, in the warning colour once the subject is longer than 50 characters. It never blocks: a longer subject commits as before.
- `commit.template` (git config) pre-fills a new commit: the first line becomes the summary, the rest the description, and lines starting with `#` are dropped, as git does before committing. A draft kept by `Esc` still comes back first, and amend and reword keep showing `HEAD`'s message.
- `[theme] base = "light"` in `config.toml` gives a palette for a light terminal: the added and removed line tints and the box around a hunk are pastel instead of near-black, the selected row's text is pure white, and a diff's syntax colours come from a light theme. `"dark"` (the default) is what ferrit always drew. An unknown base is reported and the `[theme]` section goes back to its defaults; the drawer's Save keeps the base.
- `[theme.colors]` in `config.toml` overrides any of the palette's 14 colours by name (`focus`, `idle`, `selection`, `selection_fg`, `add`, `del`, `hunk`, `hash`, `author`, `warn`, `key`, `focus_box`, `add_line_bg`, `del_line_bg`) with `"#rrggbb"` or a colour name, over the dark or light base. An unknown name is reported once and only that entry is ignored; a value that is not a colour makes the whole `[theme]` section fall back to its defaults, reported. Saving from the drawer keeps the overrides.
- An `x` menu on the Branches, Commits, Stash and Files panes holds the actions that do not earn a key: rename a branch (`r`, a popup pre-filled with the name), merge with `--no-ff` (`n`), start a branch at any commit (`b`), stash keeping the index (`i`), rename a stash (`r`), and take ours (`o`) or theirs (`t`) for a conflicted file, which still has to be staged after.
- A right click on a row selects it and opens its `x` menu (it did nothing before). Off any row, or while a popup or a confirm is up, it does nothing.
- The key hints at the bottom are clickable: a click runs the hint's action (`Fetch`, `Pull` and `Push` are clicked by their own label). Clicks on the bar are ignored while a popup or a confirm is up.
- The key hint bar and the help screen are generated from the keymap, so a remapped key shows as remapped (`Help: H` after `help = "H"`), and an unbound action loses its hint. The bar is cut to the terminal's width: segments drop from the end of the bar, and `Help` and `Quit` stay until nothing else fits. The Commits bar shows `Fetch/Pull/Push` when the terminal is wide enough (140 columns) instead of never. The help screen now lists the focused pane's keys, then the global ones, then the keys that cannot be remapped, and scrolls (`j` / `k`, `PgUp` / `PgDn`, `Home` / `End`), so every line is reachable on a short terminal; before, it was clipped below 40 rows.
- Keys can be remapped in `config.toml`: `[keys.<context>]` with `<action> = "<key>"` or a list of keys, where the context is `global`, `files`, `diff`, `branches`, `commits` or `stash`. A key is a character (`Q`), `ctrl-x` / `alt-x`, or a name (`enter`, `esc`, `space`, `tab`, `up`, `pgdn`, `f5`, ...). An entry replaces that action's default keys in that context; two actions can swap keys; an empty list unbinds. A wrong entry (unknown context or action, an action outside its own context, a key that does not parse, a key another action already has there) is reported once, keeps its action's default and does not stop the others. `ctrl-c` always quits and cannot be bound.
- Keys now go through a keymap (`src/app/keymap.rs`) instead of one long `match`. Behaviour is unchanged except for modifiers, which now have to match exactly: `Ctrl-d` outside a diff no longer opens the discard prompt, and `Ctrl-p`, `Alt-j` and the like no longer act as `p` and `j`. `Ctrl-d` / `Ctrl-u` still scroll half a page over a diff, `Ctrl-Right` / `Ctrl-Left` still switch the Branches tab, and `Ctrl-c` always quits.
- `config.toml` now holds more than the theme. `[ui]`: `mouse` (false leaves mouse capture off so the terminal's own text selection works), `wheel_step` (1 to 50, default 3) and `poll_secs` (1 to 3600, default 10). `[diff]`: `context` (0 to 200, default 3), `ignore_whitespace` and `rename_threshold` (0 to 100, default 50). `[commit]`: `sign_off` starts the commit editor with sign-off on. `[log]`: `show_reads` lists read-only commands such as `git diff` in the command log panel. An out-of-range value goes back to its default on its own and is reported; the other values in the section are kept.
- `config.toml` problems are no longer swallowed: a file that is not valid TOML, or a section that does not fit (a bad `[theme]` value), is reported once as an error toast and the Status pane, naming the section and the parser's reason, and ferrit starts on that section's defaults. Unknown sections and keys are reported and ignored, so a newer config still opens. Saving the theme from the profile drawer now keeps every other section of the file (it used to rewrite the file from the theme alone), refuses to overwrite a file that is not valid TOML, and writes through a temporary file. `ferrit --config-path` prints where the file lives.
- `P` after rewriting commits that were already pushed now says the branch has diverged (`ahead 2, behind 2`) before asking to push with `--force-with-lease`, instead of the misleading "behind upstream" wording. Nothing is pushed unless confirmed.
- Commits pane: `F` commits what is staged as a `fixup!` of the selected commit, and `a` folds every `fixup!` / `squash!` commit from the selected commit up into its target (`git rebase -i --autosquash`). `a` says so instead of rewriting when no fixup has its target inside that range. Nothing staged is an error for `F`, not an empty commit.
- Commits pane can rewrite history: `w` rewords the selected commit (the popup opens pre-filled; on the first row it still amends `HEAD`), `d` drops it after a confirm, `s` squashes it into the commit below keeping both messages, `S` fixes it up dropping its message, `e` stops the rebase at it so it can be amended. Each is one `git rebase -i` with a todo ferrit generates, no editor opened. A merge commit in the range, a dirty worktree or an operation already in progress is refused before anything changes, and a rewrite that hits a conflict or an `edit` stop leaves the rebase for the `m` menu. `w` on the Commits pane now follows the selection (it always reworded `HEAD` before). The commit editor shows no sign-off / no-verify line or `Ctrl-O/N` hint when rewording an older commit, where they never applied.
- `m` opens a menu for a merge, rebase, cherry-pick or revert git is stopped in: Continue, Skip this step (not for a merge, git has no `merge --skip`) and Abort, which asks first. Continue over an unresolved file shows git's own refusal and changes nothing; a Continue that reaches the next conflict or an `edit` stop says so. The keybar points at `m` while an operation is stopped, and the conflicted-merge note now names it instead of "phase 11".
- The Status pane now shows when git is stopped in the middle of an operation, right under the first line: `MERGING`, `REBASING 2/3`, `CHERRY-PICKING` or `REVERTING`. It is read from the repository state, so it also appears for one started from another shell, and it goes away on its own once the operation ends.
- The command log panel now shows the `git` commands ferrit really ran (the two newest writes, failures in red with their exit code) instead of two hard-coded sample lines. `@` opens a scrollable viewer with every recorded command, reads included (`j`/`k`, `PgUp`/`PgDn`, `g`/`G`, `Esc`). URL credentials are redacted; the log keeps the newest 200.
- Fixed: `<space>` and `a` no longer stage a conflicted file that still contains merge conflict markers (`<<<<<<<` and `>>>>>>>`). Before, `git add` marked it resolved with the markers inside. `<space>` now says which file to fix; `a` stages everything else and lists what it left. A resolved file, or one that only has a Markdown `=======` underline, stages normally.
- Stash pane now works: `s` on Files stashes every change (untracked included) with an optional message; on Stash, `<space>` applies, `g` pops, `d` drops after a confirm. The right pane previews the selected entry's diff. A conflicting apply or pop keeps the stash and says so.

## [0.5.0] - 2026-09-21

- Profile drawer now shows machine-global Git users as selectable radio cards. Choosing an author requires confirmation and applies to Ferrit commits without modifying Git config.
- Radio cards use fitted widths and a softened accent border for the selected user.
- Theme accent now supports Green, Blue, Purple, Amber presets and custom RGB editing from the profile drawer; selection persists in the platform TOML config.
- Source now groups Ferrit orchestration and screens under `app/`, feature logic under `domain/`: Git models and backend, profile, and image; reusable primitives and `tui_overlay` stay under `components/`.
- Profile drawer now shows settings and repository activity together in one scrollable page, including a contributor ranking by commit author and recent commits across local and fetched remote branches.
- Commit editor now follows lazygit’s summary/description flow: `Enter` commits from Summary, `Tab` switches fields, `Enter` inserts Description newlines, and `Meta`/`Ctrl-Enter` commits from Description. `Ctrl-S` remains an alias.
- Pressing `c` with an empty index now opens a `tui_overlay` backdrop confirmation to stage all changed files before committing.
- Push progress now appears inline on the checked-out branch with an animated indicator, like lazygit; Status shows it beside any earlier error.
- Pushing a branch behind its upstream now asks before using `--force-with-lease`, protecting unseen remote updates.
- `push.default=current` now pushes a new local branch without opening the upstream prompt and sets its upstream.
- Push without an upstream now opens editable `remote branch` input, including custom remote branch mapping.
- Text input cursor/editing now treats combining accents and joined emoji as one grapheme; author label alignment uses terminal cell width.
- App errors now retain typed Git/app categories and appear in Status plus a persistent bottom-right toast, dismissible with its `x` button.

- Bottom panel now has an `Infos` heading above its frame; configured Git
  author name appears inside the frame, right-aligned beside the first command.
- Clicking the configured author opens an animated right-side Git identity drawer.
- Git identity drawer lists configured author names and emails.
- Hovering the configured author shows a native pointing-hand cursor in terminals supporting OSC 22.

## [0.4.0] - 2026-09-20

### Added

- Commits pane: `Enter` on a commit row drills into that commit's changed
  files as a lazygit-style tree, replacing the commit list in place (the
  pane title becomes `[4] Diff files (<hash> <summary>)`); a directory row
  toggles collapsed with `Enter`, and the Patch pane shows the commit's full
  diff. `Esc`/`h` backs out to the commit list.
- The staged index can now be committed: `c` opens a message popup and
  creates a normal commit (disabled with nothing staged); `A` amends `HEAD`
  with the pre-filled message plus whatever is staged; `w` rewords `HEAD`'s
  message only, leaving the index untouched. The commit editor has separate
  Summary and Description fields: `Enter` commits from Summary, `Tab` switches
  fields, and `Meta`/`Ctrl-Enter` commits from Description. `Ctrl-S` remains
  an alias. `Ctrl-O` / `Ctrl-N` toggle sign-off / no-verify (no-verify shown in red, never a
  silent skip), `Esc` cancels but keeps the draft for the next `c`. Every
  commit hook, GPG/SSH signing, and `commit.*` config setting applies,
  because it's a real `git commit` subprocess; a rejecting hook or any
  other failure shows its full output in a dismissible note instead of
  silently doing nothing.
- Files pane: changes can now be staged, unstaged, and discarded, not just
  viewed. `<space>` on a file row stages or unstages it (direction inferred
  from which side has a change); `a` does the same for every changed file at
  once. `Enter` or `l` on a file row focuses the diff itself — `j`/`k` move a
  cursor over its `+`/`-` lines (context is skipped), `]`/`[` jump hunks, `V`
  starts a line selection, and `<space>` there stages/unstages the hunk under
  the cursor or the selected lines, with `--recount` handling the rewritten
  hunk header. `h`/`Esc` returns to the file list. `d` discards a worktree
  change at the same three granularities (file, hunk, or selected lines),
  always after a one-line confirm in the keybar — nothing destructive
  happens without asking first. The panes refresh immediately after any of
  this, and the diff cursor keeps its place across that refresh — even one
  triggered by a change staged from another shell.
- Branches pane: each row now shows the tip commit's age (`5h`, `1d`, `3d`,
  ...) in its own colour, lazygit-style. Just selecting a branch (no key press)
  previews its own commit log in the right pane as spaced-out `git log`-style
  blocks (hash, author, date, summary), not a cramped one-liner — scrollable
  with J/K, PageUp/Down and the mouse wheel, with its own scrollbar when it
  overflows; pressing Enter drills that same pane into that log (title
  becomes `Commits (<branch>)`) instead of the generic HEAD-based one, and
  selecting a commit there shows its diff the same way the Commits pane
  always has. `Esc` backs out to the branch list.
- Status pane: the right side now shows a lazygit-style welcome screen (a
  `ferrit` wordmark, tagline, version, licence, and a keybindings pointer)
  instead of sitting blank on a real repo. Like lazygit, the wordmark grows
  with the terminal instead of staying one fixed size: three tiers, biggest
  that fits the right pane's width and height, falling back to a plain
  `ferrit` label rather than wrapping a wordmark into noise below all three.
  Identical in `App::mock()` and against a real repo — it's app chrome, not
  repo data.
- Files pane: changed paths below the repo root now group into a lazygit-
  style directory tree (a root `/` row, one header per directory, files
  shown by their own name once nested) instead of a flat list of full
  paths. Directories toggle collapsed/expanded with Enter or a left click
  on the row (lazygit's own click-to-toggle, not just a keybinding). Stays
  exactly the previous flat list — no root row, no headers — when every
  changed file is directly at the repo root, which is most working trees
  most of the time.
- Files pane: selecting a changed file now shows both sides at once,
  lazygit-style — an "Unstaged Changes" column beside a "Staged Changes"
  one — instead of a single diff that guessed which side to show. The left
  column narrows while this split is up so both stay readable. A file with
  changes on only one side just shows an empty diff on the other.
- Branches pane: `HEAD` can now move. `<space>` checks out the selected
  branch; `n` opens a popup for a new branch's name, always branched from
  the current `HEAD`; `d` deletes the selected branch after a one-line
  confirm, asking a second time (to force it) if it turns out to be
  unmerged, and refuses outright (no confirm) on the branch that is
  currently checked out; `u` fast-forwards the selected branch to its
  upstream whether or not it's the one checked out; `M` merges the
  selected branch into the current one, landing a merge commit, a
  fast-forward, or a conflicted state git itself would also leave — visible
  in the Files pane and a dismissible note, not silently pretended away.
  Every action shells out to real `git`, so hooks and git's own safety
  messaging (a dirty worktree a checkout would clobber, an unmerged
  delete's refusal) apply exactly as they would from a shell.
- ferrit can now talk to a remote: `f` fetches, `p` pulls (honouring
  whatever `pull.rebase`/`pull.ff` config is already set), and `P` pushes —
  offering to set an upstream via a small remote picker when the current
  branch has none. All three run on a background thread, so a slow or
  stalled network never freezes the keyboard; a "Fetching…"-style label
  shows while one is in flight, a short confirmation line once it's done,
  a failure with git's own message otherwise. The Branches pane gains a
  real Remotes tab (`Ctrl-Right`/`Ctrl-Left` to switch to it) listing every
  configured remote's fetch and push URLs.

### Changed

- Manual and automatic repository refreshes now read snapshots on a worker
  while the TUI stays responsive. Bursts coalesce into one follow-up refresh.
- Selected file diffs, commit diffs and branch logs load off-thread. Fast
  navigation keeps only one active read and latest pending selection; stale
  results cannot replace the current preview.
- Image blob reads and decoding, plus refreshes of drilled branch/commit data,
  now run off-thread too. Old image results cannot replace a newer selection.
- Commit and branch popups now share reusable `TextInput` and `Dialog`
  components; remote, note and help overlays reuse the same dialog shell.
- Panes, remote selection and key-hint rows now use shared `Panel`,
  `SelectList` and `KeyBar` components with existing styling preserved.
- Pane-list state/scroll rendering and preview scrollbars now use shared
  `PaneList` and `ScrollBar` components.
- Popup rendering is isolated in `src/ui/popups.rs`; shared dialogs support
  content-sized layouts and compose existing `TextInput`, `SelectList`, and
  `KeyBar` components.
- The event loop now handles bounded batches of queued input and worker events,
  avoiding a repaint for every key-repeat while preserving event order.
- Refresh restores selection by stable file, directory, branch, commit, or
  stash identity; stash rows now carry their object id for reliable matching.
- Background refresh, diff, image, and remote workers report panic failures
  through their normal completion events, releasing their in-flight state.
- Remote Git commands now have a five-minute deadline and stop their process
  group when Ferrit shuts down, keeping captured diagnostics on timeout or
  cancellation; typed worker kinds replace string labels.
- A failed filesystem watcher no longer prevents startup; Ferrit keeps polling
  and shows the watcher failure in the Status pane.
- Two-sided Files diff rendering now lives in `src/ui/diff.rs`, separate from
  the screen layout and shared overlays.
- Component and Git APIs now use explicit module paths; `clippy::pub_use` is
  denied crate-wide to prevent re-export shortcuts from returning.
- Every bordered box (left panes, right pane, command log, help overlay,
  image preview) now uses rounded corners (`╭╮╰╯`), matching lazygit's own
  look, instead of ratatui's square-corner default (`┌┐└┘`).

### Fixed

- Terminal initialization failures now unwind raw mode/alternate-screen changes;
  restoration always attempts to disable raw mode even if screen cleanup fails.
- Left column accordion: the focused pane now claims a weighted majority of
  the space (4 shares vs. 1 for each other pane) instead of a fixed floor
  each with 100% of the leftover to focus. The old scheme fell back to a
  perfectly even split — no accordion at all — whenever the terminal was too
  short for every pane's floor, which is exactly when a clear size
  difference matters most; the new one degrades gracefully at any height.
  Status is also sized to its actual line count (3 normally, 4 with a
  conflict to report) instead of a flat 4, freeing a row for the others.
- Branch recency and the branch-log preview's `Date:` line always floored to
  whole days, so anything committed earlier the same day showed a misleading
  `0d ago` instead of a real age. Both now step down to hours, minutes, or
  seconds once the elapsed time is under a day (`4h`, `12m`, `9s`).

## [0.2.0] - 2026-09-11

### Added

- Diff renderer now matches target lazygit/lazygitrs pager treatment: context
  lines keep syntax colours on plain background; `+`/`-` lines use flat
  add/delete colours with a full-width background; when available, delta now
  supplies the Patch layout, line-number gutter, word highlights, file
  markers, separators, and stat block.
- Styled diff `Text` now caches in `App`; scroll-only redraws reuse rendered
  spans. Cache invalidates on diff content, selection, focus anchor, or pane
  width changes.
- `docs/TESTS_STRATEGY.md`: maps lazygit's 551-file integration test suite
  onto our phase table as a `.script` behavior backlog, companion to
  `PLAN_SELF_TESTING.md`.
- Every left-column pane (Status, Files, Branches, Commits, Stash) now draws a
  vertical scrollbar when its list overflows the pane, matching the right
  pane's diff scrollbar; the thumb is green while the pane is focused and grey
  otherwise, following the pane's own border colour.
- Left column accordion: the focused pane among Files/Branches/Commits/Stash
  now grows to claim the leftover vertical space, the other three collapse to
  a 3-row floor (border + one row), lazygit-style. Status stays fixed height
  regardless of focus. Computed by hand rather than via `ratatui::Layout`,
  which mixes `Min`/`Fill` in an order-sensitive way at small heights.
- Diff and commit view: a lazygit-style `old new│` line-number gutter in front
  of every line, derived from the hunk header counters already parsed
  (`Diff::line_numbers`). Blank on headers, one-sided on an addition or
  deletion, both columns on context.
- Diff and commit view: a `git --shortstat` style summary line (`N file(s)
  changed, X insertion(s)(+), Y deletion(s)(-)`) above the scrollable diff,
  derived from `Diff::stat`. Rendered as its own row so it does not shift the
  line-index alignment scroll and hunk/file focus rely on.
- Diff and commit view: a `]` / `[` jump now boxes the whole hunk (or file, in
  a commit) in a dim background tint, not just its header line, so the
  boundary a jump landed on stays visible even after scrolling the header out
  of view.
- Diff and commit view: per-language syntax highlighting on every code line
  (`syntect`, syntax picked from the file's extension via `Diff::line_extensions`),
  plus a full-line pastel green/red background tint (`ADD_LINE_BG`/
  `DEL_LINE_BG`) on `+`/`-` lines so an addition or deletion still reads at a
  glance under the syntax colours. Metadata lines (headers, hunk markers,
  binary/no-newline notices) keep the flat `diff_line_style` colouring.
- Left click on the right pane focuses it (border lights up like a left
  pane's); `Esc` returns focus to the left column. Click still routes
  scrolling exactly as before.
- Left click on a left-pane row focuses that pane and moves its selection
  cursor to the clicked row (lazygit style), rebuilding the right pane the
  same way a `j` / `k` move would. Clicking a pane's border or title focuses
  it without moving the cursor; a click past the last row, on the command
  log, or in a gap does nothing; any click dismisses the help overlay.
  Right click, middle click, drag and mouse move stay inert for now.
- Strict Rust tooling, ported from the RUSTIFY reference setup. `rust-toolchain.toml`
  pins the compiler (1.97.1) so CI and every contributor lint identically.
  `rustfmt.toml` and `clippy.toml` fix formatting and the MSRV clippy target.
  `Cargo.toml` grows `[lints.clippy]`, `[lints.rust]` and `[lints.rustdoc]` tables:
  the clippy `all` / `pedantic` / `nursery` / `cargo` groups run at warn, a curated
  deny list breaks the build on `unwrap`, `panic`, `todo`, `indexing_slicing`,
  `print_stdout` / `print_stderr`, `exit`, undocumented `unsafe` and more, and
  `unsafe_code` is `forbid`. `deny.toml` adds a `cargo deny check` gate over
  advisories, licences, banned and duplicate deps, and the source allowlist.
- `.github/workflows/ci.yml`: runs `cargo fmt --check`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `RUSTDOCFLAGS=-D warnings cargo doc`,
  `cargo machete`, `cargo deny check` and `cargo nextest run` on every push to
  `main` and every pull request.

### Fixed

- Right-pane diff scrollbar: the thumb now reaches the track's bottom at max
  scroll instead of stopping one cell short. `ScrollbarState`'s
  `content_length` is the count of distinct scroll positions
  (`total - viewport + 1`), not the raw line count, so a thumb sized against
  the raw total never spanned the full track.
- Both scrollbars drop their begin/end arrow glyphs (`.begin_symbol(None)`,
  `.end_symbol(None)`): a plain track + thumb, lazygit style, instead of
  arrows eating a row at each end.

### Changed

- `Repo::file_diff()` and `Repo::commit_diff()` take `DiffOpts` by value instead
  of by reference. `DiffOpts` is a 12 byte `Copy` struct, so the reference was
  pure overhead (`clippy::trivially_copy_pass_by_ref`).
- Lint fallout across `src/`: `map_or_else` instead of `map(..).unwrap_or_else(..)`,
  checked `usize` / `isize` arithmetic instead of `as` casts in the scroll paths,
  slice `.get()` instead of indexing, and small scoped `#[expect(..)]` where a
  lint flags a provably unreachable arm or a disproportionate dependency.

### Removed

- Unused direct dependency `crossterm`. Only the `ratatui::crossterm` re-export
  was ever used.

## [0.1.0] - 2026-09-08

### Added

- Right-pane scroll, lazygit style. The diff view now scrolls without leaving
  the focused left pane: `J` / `K` by a line, `PageUp` / `PageDown` by a page,
  `Ctrl-u` / `Ctrl-d` by a half page, `<` / `>` to the ends, `]` / `[` between
  hunks (or files, in a commit). Step sizes follow the real pane height. A
  vertical scrollbar shows on the right pane whenever the diff overflows, its
  thumb tracking the scroll position. The mouse wheel scrolls whichever pane
  the pointer is over (mouse capture is now enabled). The scroll keys and the
  wheel are inert over an image, a "no changes" note, and the mock bodies. New
  keys are listed in the keybar and the `?` help overlay.
- Real diff view in the right pane, lazygit style. Selecting a Files row runs
  `git diff` (or `git diff --cached` for a fully staged file, `git diff
  --no-index` for an untracked one); selecting a Commits row runs `git show`.
  Output is coloured from a byte-range parser (`src/git/diff/`): hunk headers
  cyan, additions green, deletions red, file and commit metadata bold. Vertical
  scroll with `Ctrl-d` / `Ctrl-u`, jump between hunks (or files, for a commit)
  with `]` / `[`. The viewport is kept across a background refresh of an
  unchanged selection and reset to the top when the selection moves. An image
  selection still owns the pane. `git config` (`diff.algorithm`, rename
  detection, and so on) is honoured because the diff is a subprocess.
- `Repo::file_diff()` and `Repo::commit_diff()` on the read-only git backend,
  plus `git::parse_diff()` for plain-text diff parsing without a subprocess.
- Live auto refresh, lazygit style. A background event multiplexer
  (`src/events.rs`) feeds the render loop from three sources: terminal input, a
  recursive filesystem watch on the worktree, and a 10 second poll fallback.
  Staging or committing from another shell, or an editor saving a file, now
  updates the panes on its own with no keypress. Filesystem bursts are
  debounced (150 ms) and `*.lock` plus `.git/objects/` churn is filtered so one
  stage triggers exactly one refresh. Manual `r` still works.
- `Repo::workdir()` accessor, exposing the worktree root that the watcher
  recurses from.

### Fixed

- Selection bar, lazygit style: the solid blue row highlight now shows only in
  the focused left pane. Unfocused panes keep their cursor position but draw no
  bar, so the three panes no longer look selected at once.
- Right pane with a real repo and nothing selected (fresh repo, no commits, no
  changes) showed `App::mock()`'s hardcoded sample diff instead of staying
  blank. `App::is_mock()` now gates that fallback to the repo-free path only.

[Unreleased]: https://github.com/ferrit-rs/ferrit/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/ferrit-rs/ferrit/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/ferrit-rs/ferrit/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/ferrit-rs/ferrit/releases/tag/v0.4.0
[0.2.0]: https://github.com/ferrit-rs/ferrit/releases/tag/v0.2.0
[0.1.0]: https://github.com/ferrit-rs/ferrit/releases/tag/v0.1.0
