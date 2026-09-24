# Plan: phase 12, polish

**Shape: a bucket, cut into slices.** `docs/PLAN_0_GENERAL.md` lists five
things under this phase (config file, themes, keymap customization, real
command-log capture, help). The plan SOP (`__SOP/create-new-plan-file.md`)
says one phase is one mergeable slice, and these five are not one slice: they
depend on each other in a fixed order. So this plan keeps one file but ships
as seven independently mergeable slices, P0 to P6 (P7 is the closing polish),
each with its own tests and changelog line. Stopping after any of them leaves
ferrit working.

## Goal

Make ferrit configurable and self-explaining without growing the default
keymap: a config file that can hold more than a theme, a keymap the user can
change, a help screen and key hints that cannot drift from the real
bindings, a command log that shows the real `git` commands, and the `x` menu
that `PLAN_0_GENERAL.md` promised for the long tail ("Small keymap. Menus
(`x`) hold the long tail.").

lazygit is the model for all of it (`config.yml`, its `keybinding` section, the
`@` command-log menu, the `x` options menu). The reference checkout
(`../ferrit-references/`) is absent here, so no `path:line` is cited and the
lazygit behaviours are from its published docs, not re-checked in source.

## The gap this fixes

What exists today, each read from the code:

- **Config.** `src/app/theme_config.rs` owns `config.toml`
  (`ProjectDirs::from("dev", "Ferrit", "Ferrit")`), and the file is
  `struct ConfigFile { theme: ThemeConfig }`. Two problems:
  - `load` swallows every error with `.ok()`: a typo silently gives the
    default theme.
  - `save` rewrites the whole file from `ConfigFile`, so the first non-theme
    section anyone adds is destroyed by the next theme save.
- **Tests read the developer's real config.** `App::open` calls
  `ThemeConfig::load()` (`src/app/mod.rs:719`), so every integration test that
  opens an app depends on the file in the developer's config directory. Harmless
  for a theme colour, breaking once a keymap can live there.
- **Keymap.** About 68 `KeyCode` arms in `src/app/input.rs`, a few named
  constants (`KEY_CONFIRM_YES`, `KEY_NAV_*`, the theme keys), the rest literal
  characters. No indirection, so nothing to remap.
- **Help and key hints are hand-written strings.** `mock::HELP` and
  `mock::KEYBAR` / `BRANCHES_KEYBAR` / `STASH_KEYBAR` already drifted: in
  phase 10 the default keybar could not take `Stash: s` (127 columns against
  the 120 the render tests hold it to) and three help lines had to be merged
  to fit 40 rows. The help dialog also clips: `Dialog::render` clamps to the
  area (`src/components/ui/dialog.rs`) and nothing scrolls, so below 40 rows
  the last bindings are unreachable.
- **The command log is fake.** `src/app/screens/mod.rs`
  (`draw_command_log`) prints `mock::COMMAND_LOG`, two hard-coded strings
  (`$ git status --porcelain`, `$ git diff src/main.rs`), in every repo,
  forever. There are eight `Command::new("git")` sites
  (`apply.rs` 2, `branch.rs` 2, `commit.rs`, `diff.rs`, `remote.rs`,
  `stash.rs`), and phase 11 adds more.
- **Hard-coded knobs** that earlier plans promised to move to config:
  mouse master toggle and wheel step (`WHEEL_LINES = 3`, `PLAN_5`), the 10 s
  poll (`POLL_INTERVAL` in `src/app/events.rs`), diff context and whitespace
  (`DiffOpts::default()`, which only mirrors lazygit's defaults), sign-off
  default (`PLAN_7`).
- **Palette.** `src/app/theme.rs` has 14 colour constants (`FOCUS`, `IDLE`,
  `SELECTION`, `ADD`, `DEL`, `HUNK`, `HASH`, `AUTHOR`, `WARN`, `KEY`,
  `FOCUS_BOX`, `SELECTION_FG`, `ADD_LINE_BG`, `DEL_LINE_BG`), about 42 uses
  outside `theme.rs`. Most are ANSI names that follow the terminal's own
  theme; only the two `Rgb` line backgrounds and `SELECTION_FG` assume a dark
  terminal.

## Order and dependencies

```
 P0 command log  ----------------------------------.
   (independent; best landed BEFORE phase 11,       |
    which adds the most git subprocesses)           |
                                                    v
 P1 config foundation --> P2 keymap --> P3 help and hints generated from it
        |                     |                   |
        |                     '-----> P4 x menu, right-click, clickable hints
        |                                   (needs phase 11's Popup::Menu)
        '--> P5 palette and themes
        '--> P6 commit and remaining knobs
```

## P0: real command log

**One choke point.** Every subprocess goes through one function in a new
`src/domain/git/exec.rs`:

```rust
pub(super) fn run(workdir: &Path, args: &[OsString], env: &[(&str, OsString)])
    -> io::Result<Output>;
```

The eight existing sites call it instead of `Command::new("git")` (a
mechanical change, one commit per module), and phase 11's new sites start
there. It records a `CommandRecord`:

```rust
pub struct CommandRecord {
    pub argv: String,       // "git checkout -b wip/x", credentials redacted
    pub kind: CommandKind,  // Read (diff, show, rev-list, stash show) or Write
    pub exit: Option<i32>,  // None when git could not be spawned
    pub took: Duration,
}
```

**Where it lives.** A process-wide ring buffer (`OnceLock<Mutex<VecDeque<_>>>`,
200 entries) in `exec.rs`. A global is the honest choice here: the diff and
refresh workers each `Repo::open` their own handle on their own thread
(`src/app/diff_query.rs`), so a handle owned by one `Repo` would never see
their commands. `git::command_log::recent(n)` reads it; nothing else can
write it. The crate denies `unwrap` / `expect` (`Cargo.toml`), so the lock
is taken with `lock().unwrap_or_else(PoisonError::into_inner)`: a panic in
one worker must not blind the log.

**Reads versus writes.** `git diff` runs on every selection change, so
logging it would bury the commands the user actually asked for. Default shows
only writes; `[log] show_reads = true` shows everything. `git2` reads are not
subprocesses and are never logged (lazygit would show them; this is a
documented difference, not a bug).

**Redaction.** Any `scheme://user:secret@host` in an argument is logged as
`scheme://user:***@host`. Commit messages passed with `-m` are logged as
given: they are the user's own text, shown on the user's own screen.

**Display.** `draw_command_log` reads `recent(2)` instead of
`mock::COMMAND_LOG`. Failures are drawn in the error colour with the exit
code. The `👤 author` label keeps its place on the first row
(`draw_command_log`, unchanged). `App::mock()` keeps `mock::COMMAND_LOG` so
render tests stay repo-free.

**Viewer.** Two rows cannot show a multi-command operation, so `@` opens a
scrollable `Popup::CommandLog` with the whole ring (newest last, `j` / `k` /
`PgUp` / `PgDn`, `Esc`). `@` is unbound today.

Out of P0: streaming stdout, timing histograms, copy to clipboard.

## P1: config foundation

One `Config` owns the file and every section, in a new `src/app/config/`
module (`mod.rs`, plus `keys.rs` from P2). `ThemeConfig` moves in as the
`theme` field, same TOML shape (`[theme]` with `preset` and `accent`) so
existing files keep working.

```toml
[theme]            # exists today
preset = "green"
accent = "#3fb950"

[ui]
mouse      = true   # false: do not capture the mouse (terminal selection works)
wheel_step = 3      # WHEEL_LINES today
poll_secs  = 10     # POLL_INTERVAL today

[diff]
context           = 3      # DiffOpts.context
ignore_whitespace = false
rename_threshold  = 50

[commit]
sign_off = false   # PLAN_7 "Sign-off default"

[log]
show_reads = false # P0
```

Rules:

- **Errors are surfaced, not swallowed.** A file that does not parse, or a
  value out of range (`wheel_step = 0`), starts ferrit on defaults and shows
  one startup notice and a toast naming the file and the reason. Unknown keys
  are reported once, not rejected, so a newer config still opens in an older
  ferrit.
- **Saving merges.** `Config::save` reads the file as a `toml::Table`,
  replaces only the sections ferrit owns, and writes it back, so sections it
  does not know survive. Comments are lost (the `toml` crate does not
  preserve them, and `toml_edit` would be a new dependency for a feature
  nobody asked for). This is documented in the file's own header comment,
  which ferrit writes on first save.
- **Tests never read the real file.** `App::open(path)` uses
  `Config::default()`. The binary calls a new `App::open_with(path, Config)`
  with `Config::load()`. `Config::from_str` is the seam tests use. This also
  fixes today's dependence on the developer's own config.
- **Precedence:** defaults, then the file. The existing `FERRIT_*`
  environment variables for graphics (`src/domain/image/detect.rs`) stay
  environment-only.
- **`ferrit --config-path`** prints where the file is and exits. Cheap, and
  the first thing a user needs to know.

## P2: keymap

An `Action` enum names every bound behaviour, and a `Keymap` maps
`(Context, KeyBinding) -> Action`:

```rust
pub enum Context { Global, Files, Diff, Branches, Commits, Stash, Menu }
pub enum Action { Quit, Help, Refresh, StageFile, StageAll, Commit, Amend, Reword,
                  Discard, Fetch, Pull, Push, /* .. one per binding today .. */ }
```

`input.rs`'s `match key.code` arms become `keymap.action(context, key)` plus a
`match action`. Defaults are exactly today's bindings, proved by a test that
replays every existing key test through the default keymap.

```toml
[keys.global]
quit = "q"
refresh = "r"

[keys.stash]
drop = "D"          # remap one action in one context
```

- **Key syntax:** a single character, or `ctrl-x` / `alt-x` / `shift-tab` /
  named keys (`enter`, `esc`, `space`, `up`, `pgdn`, `f5`). No chords in this
  phase.
- **Contexts resolve most specific first:** the focused pane's context, then
  `global`. That is how `d` means discard on Files and delete on Branches
  today, without a special case.
- **Load-time validation:** a key bound twice in one context, an unknown
  action name or an unparseable key is reported (P1's notice) and that one
  entry falls back to its default; the rest of the file still applies.
- **Not remappable, on purpose:** text entry inside popups, `Esc` / `Enter`
  in popups, and the `y` / `n` confirm answers. A user who rebinds `y` should
  not be able to make a destructive confirm unanswerable.
- **Mouse is not in the keymap.** Click and wheel behaviour stay as in
  phases 4 and 5.

Out of P2: chords and sequences, per-repo config, a rebinding UI.

## P3: help and hints generated from the keymap

`mock::HELP`, `KEYBAR`, `BRANCHES_KEYBAR`, `STASH_KEYBAR` and the phase 11 menu
hints become output of `Keymap`, so they cannot disagree with the real
bindings or with a user's remaps.

- Each `Action` carries a short label and a `hint: bool` (does it earn a
  keybar segment). The keybar per context is built from the hinted actions in
  priority order and **drops trailing segments to fit the terminal width**,
  instead of the hand-trimmed 116-column string that had no room for
  `Stash: s`.
- The help overlay lists every action of the current and global contexts,
  grouped, and **scrolls** (`j` / `k` / `PgUp` / `PgDn`) so a 24-row terminal
  reaches every line. This removes the clipping described above.
- `App::mock()` keeps a fixed default keymap, so render tests are unchanged.

## P4: `x` menu, right-click, clickable hints

Phase 11 builds `Popup::Menu` (title, items with a label, shortcut and
action, `j` / `k`, `Enter`, `Esc`) and uses it for `m`. This slice adds the
per-pane `x` menu that holds the long tail, seeded with the loose ends earlier
plans parked in "Out of scope". Each is one `git` command, none needs a new
design:

| Pane | Entry | Command | Parked in |
| --- | --- | --- | --- |
| Branches | rename branch | `git branch -m <old> <new>` (name popup) | `PLAN_8` |
| Branches | merge with `--no-ff` | `git merge --no-ff` | `PLAN_8` |
| Commits | new branch from this commit | `git checkout -b <name> <hash>` | `PLAN_8` |
| Stash | push keeping the index | `git stash push --keep-index` | `PLAN_10` |
| Stash | rename stash | store under a new message, then drop the old | `PLAN_10` |
| Files | take ours / take theirs (conflicted file) | `git checkout --ours|--theirs -- <path>`, then it still needs marking resolved | `PLAN_11` |

Right-click on a row opens the same menu for that row (`PLAN_5` left it as an
explicit `// phase 12` no-op in `Down(MouseButton::Right)`). The keybar
hints become clickable: a click on a segment dispatches its `Action`, which
is why this slice follows P2 and P3.

Out of P4: tags tab (`PLAN_8` says "revisit in `PLAN_12_POLISH.md` or its own
follow-up"; it is a whole feature, not polish, and stays unscheduled),
checkout from a remote-tracking ref, `git remote add/remove/rename`, fetch
progress (`PLAN_9`).

## P5: palette and themes

`theme.rs`'s 14 constants become one `Palette` struct held on `App`
(`app.palette()`), with two built-ins and per-colour overrides:

```toml
[theme]
preset = "green"        # the accent, as today
base   = "light"        # "dark" (default) or "light"

[theme.colors]          # any subset of the 14 names, "#rrggbb" or an ANSI name
add_line_bg = "#d6f5d6"
```

`dark` is today's palette, unchanged. `light` differs only where the current
palette assumes a dark terminal: `ADD_LINE_BG`, `DEL_LINE_BG`,
`SELECTION_FG`. The other colours are ANSI names that already follow the
terminal's theme. The accent keeps its existing meaning (presets and the RGB
picker in the profile drawer), and saving from the drawer must go through
P1's merging save so it stops being the only writer.

About 42 call sites change from `theme::ADD` to `palette.add`; the change is
mechanical and lands in one commit. Out of P5: theme files in a themes
directory, live theme switching beyond the existing drawer, 30+ presets
(`INSPIRATION.md`: lazygitrs).

## P6: commit and remaining knobs

Small settings the earlier plans parked here:

- `[commit] sign_off` defaults the popup's sign-off toggle (`PLAN_7`
  "Sign-off default": still visible in the footer, still flipped per commit
  with `Ctrl-O`).
- `commit.template`: when git config names one and the message is empty,
  the commit popup starts from it (`PLAN_7`).
- Subject length indicator in the commit popup: a `n/50` counter that turns
  warning-coloured past 50, no blocking (`PLAN_7` "50/72 lint").
- `mouse`, `wheel_step`, `poll_secs`, `[diff]` wired from P1 into the places
  that hard-code them today.

Out of P6: opening `$EDITOR` for the message. The terminal is in raw mode on
an alternate screen; suspending it to spawn an editor is its own piece of
terminal-lifecycle work with its own failure modes, not a config line.

## Edge cases

| Case | Behaviour |
| --- | --- |
| config file missing | defaults, no notice |
| config file unparseable | defaults, one notice and toast with the parse error and line |
| unknown section or key | ignored, reported once; preserved on save |
| two actions on one key in one context | that entry falls back to its default, reported |
| user remaps `q` and `Ctrl-c` is still reserved | `Ctrl-c` always quits, it is not in the keymap |
| terminal narrower than the keybar | trailing segments are dropped, never wrapped |
| command log while `git` cannot be spawned | record with `exit: None`, shown as an error line |
| command log ring full | oldest entries drop, the viewer shows the last 200 |
| `mouse = false` | mouse capture not enabled, click and wheel inert, keyboard unaffected |
| theme saved from the drawer with `[keys]` present | other sections preserved |

## Out of scope

- Chords and key sequences, a rebinding UI, per-repository config.
- Tags tab, remote management, checkout of remote-tracking refs, fetch
  progress, `$EDITOR` integration.
- Undo built on the reflog, worktree management, PR / issue panel, AI
  commit messages: `PLAN_0_GENERAL.md` "Not scheduled (revisit after phase
  12)". Unchanged by this plan.
- Windows. `PLAN_0_GENERAL.md` names Linux and macOS for v1.0.

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/git_exec.rs` (P0): each write goes through `exec::run` and is
  recorded with the right argv, kind and exit code; a failing command is
  recorded with its exit code; redaction of `https://u:p@h`; ring cap;
  reads hidden by default.
- `tests/config.rs` (P1): each section round-trips; a partial file fills the
  rest with defaults; a syntax error and an out-of-range value produce a
  notice and defaults; `save` preserves an unknown section; `App::open` never
  reads the real config directory (points `XDG_CONFIG_HOME` / `HOME` at an
  empty temp dir and asserts the file is not opened).
- `tests/keymap.rs` (P2): the default keymap reproduces every existing
  binding (table-driven from the current `input.rs` arms, written before the
  refactor); a remap moves one action; a duplicate key falls back with a
  report; reserved keys reject a remap; context precedence.
- `tests/render.rs` (P3, P4): the keybar at 120, 80 and 60 columns drops
  segments instead of wrapping; help scrolls and shows its last line at 24
  rows; a remapped key appears in help and keybar; the `x` menu and the
  command-log popup render.
- `tests/app_menu.rs` (P4): each seeded entry runs its command and refreshes;
  right-click opens the menu for the clicked row; a keybar click dispatches.
- `tests/render.rs` (P5): the `light` base changes the three colours and
  nothing else; an override applies.
- Prior phases stay green in every slice: the keymap refactor in particular is
  gated by the whole existing `tests/app_*.rs` suite passing unmodified.

## Milestones

- **P0a** `exec.rs` and the ring buffer, no callers changed. Unit tests.
- **P0b** the eight call sites move to `exec::run`, one commit per module,
  suite green after each.
- **P0c** `draw_command_log` reads the ring; `@` viewer; `[log] show_reads`
  waits for P1 and defaults to off meanwhile.
- **P1a** `Config` with today's `theme` section only, error surfacing, merging
  save, `App::open` versus `open_with`, `--config-path`. Behaviour identical.
- **P1b** `[ui]`, `[diff]`, `[commit]`, `[log]` sections parsed and wired to
  the constants they replace.
- **P2a** `Action`, `Context`, `Keymap::default()` and the table test, no
  behaviour change yet.
- **P2b** `input.rs` routes through the keymap; every existing test unmodified
  and green.
- **P2c** `[keys]` parsing, validation, fallbacks.
- **P3** generated keybars and scrollable help.
- **P4** `x` menu with the six seeded entries, right-click, clickable hints.
- **P5** `Palette`, `dark` / `light`, `[theme.colors]`.
- **P6** commit and remaining knobs.
- **P7** polish: `cargo clippy --all-targets` clean, `cargo fmt --check`,
  layering held (`src/domain/git/` has no `ratatui`), every edge-case row
  tested or explicitly inert, `CHANGELOG.md` lines, `PLAN_0` flipped.

## Definition of done (phase 12)

- The command log shows the commands ferrit really ran, failures in the error
  colour, and `@` shows the whole history; no mock string remains outside
  `App::mock()`.
- One `config.toml` holds theme, UI, diff, commit, log and key settings; a
  broken file never crashes ferrit and never silently applies half a
  configuration; saving never destroys a section it does not own.
- Any binding except the reserved ones can be remapped per context, and help
  and keybars show the remapped keys.
- Help reaches every binding on a 24-row terminal.
- `x` and right-click open the per-pane menu; the six seeded entries work.
- A `light` base and per-colour overrides exist without changing the default
  look.
- Tests never depend on the developer's own config directory.
- `src/domain/git/` still has no `ratatui` import.
- `cargo clippy --all-targets` clean; the new test files and every earlier
  test file pass.

## After phase 12

`PLAN_0_GENERAL.md`'s "Not scheduled" list is next: undo on the reflog, AI
commit messages, a PR / issue panel, worktrees. Two things this phase leaves
as hooks for them: `Action` (a new feature is a new action plus a menu
entry, not a new key arm) and the command log (an undo view reads the same
ring to know what to reverse).
