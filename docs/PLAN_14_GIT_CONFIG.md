# Plan: phase 14, git config editor

**Status: in progress (G0 and G1 done: `src/domain/git/config.rs` reads, parses and writes; no screen yet).** lazygit has no git config screen, so
this phase is not compared with it; it is checked with the replay harness and
screenshots (`PLAN_SELF_TESTING.md`).

## Goal

View and change the git configuration from inside ferrit: what is set, at which
level (local repository or global), what wins when a key is set twice, and a safe
way to edit, add or remove a key without opening `~/.gitconfig` in an editor.
Today the only writes ferrit offers are the identity choice in the profile drawer
and its own `config.toml` (`PLAN_12_POLISH.md` P1, which is a different file:
ferrit's settings, not git's). Everything else (`pull.rebase`, `push.default`,
`commit.gpgsign`, `core.editor`, aliases) means leaving for the shell.

```
┌ Git config ─ scope for changes: [L]ocal ─ 41 keys ── filter: pull ┐
│ S  key                        value                               │
│ G  pull.rebase                true                                │
│ L  pull.rebase                merges          ← wins (local)      │
│ G  pull.ff                    only                                │
│ ── core ─────────────────────────────────────────────────────     │
│ G  core.editor                nvim                                │
│ S  core.autocrlf              input                (system, r/o)  │
│ ── credential ───────────────────────────────────────────────     │
│ G  credential.helper          osxkeychain                         │
│ G  url.git@github.com-personal:.insteadof  https://github.com/    │
└ e: edit  a: add  d: unset  Space: toggle  s: scope  /: filter  Esc ┘
```

Rows are `[scope] key value`. A key set at several levels shows every value; the
one git uses is marked, the shadowed ones are dimmed.

Out, on purpose (see "Out of scope"): editing the system file, editing an
included file in place, ferrit's own `config.toml`.

## The gap this fixes

- `git::Repo::user_name` and the profile module read identities; nothing lists
  the whole configuration, and no code writes a git key except the identity
  choice.
- The keys that decide how ferrit itself behaves are git's: it deliberately
  honours `pull.rebase`, `push.default`, `commit.gpgSign` and `diff.*`
  (`PLAN_7_COMMIT.md`, `PLAN_9_REMOTE.md`). A user cannot see or change them
  where they take effect.

## Approach

**Shell out to `git config` for every read and write.** Same rule as phases 6 to
11: git owns the file format, includes, `includeIf`, quoting and locking, and
ferrit must not rewrite `~/.gitconfig` itself. Every call goes through
`exec::git` (`src/domain/git/exec.rs`) so it appears in the command log.

**Reading**, one call: `git config --list --show-origin --show-scope -z`
(needs git 2.26 for `--show-scope`; on an older git fall back to
`--show-origin` and infer the scope from the file path, and say so in the footer).
The parser is written against real output and unit-tested with captured samples,
including the NUL/newline separators of `-z`, values containing newlines, empty
values and a key with no `=`.

**Writing**, one call per change, always with an explicit scope flag so nothing
lands in the wrong file:

| Action | Command |
| --- | --- |
| set | `git config --local\|--global <key> <value>` (`--worktree` only when `extensions.worktreeConfig` is on) |
| set a typed value | the same with `--type=bool\|int\|path` so git validates and normalises it |
| add a value to a multi-valued key | `git config --add <key> <value>` |
| replace every value | `git config --replace-all <key> <value>` |
| unset | `git config --unset-all --local\|--global <key>` |

After each write ferrit re-reads the list, keeps the selection on the same key,
and refreshes what it derived from config (the identity in
`profile.settings`, `push_default_current`, the header). That refresh is the same
`request_refresh` every other write ends with.

## Backend: `src/domain/git/config.rs`

New module (git adapter, no `ratatui`), beside `remote.rs` and `branch.rs`:

```rust
pub struct ConfigEntry { pub scope: Scope, pub origin: Origin, pub key: String, pub value: String }
pub enum Scope { System, Global, Local, Worktree, Command }
pub struct ConfigView { pub entries: Vec<ConfigEntry> }   // every value, in git's order
impl ConfigView {
    pub fn effective(&self, key: &str) -> Option<&ConfigEntry>;   // the last one wins
    pub fn shadowed(&self, key: &str) -> impl Iterator<Item = &ConfigEntry>;
}
pub(super) fn read(repo: &Repository) -> GitResult<ConfigView>;
pub(super) fn set(repo: &Repository, scope: WriteScope, key: &str, value: &str, kind: ValueKind) -> GitResult<()>;
pub(super) fn add(...);   pub(super) fn unset(...);
```

- `WriteScope` has only `Local`, `Global` and `Worktree`: the type makes writing
  to `System` or an include file impossible, not merely refused at runtime.
- A **known-keys table** (`KNOWN_KEYS`) gives a type and the allowed values for
  the keys people actually change: booleans (`commit.gpgsign`, `fetch.prune`,
  `push.autoSetupRemote`, `rebase.autosquash`, `rebase.autostash`), enums
  (`pull.rebase`: false, true, merges, interactive; `pull.ff`; `push.default`;
  `merge.conflictstyle`; `core.autocrlf`; `diff.algorithm`; `gpg.format`),
  strings (`user.name`, `user.email`, `user.signingkey`, `core.editor`,
  `init.defaultBranch`). An unknown key is still editable as free text; the
  table only adds toggles and a picker.
- **Validation is git's.** A bad value (`--type=int` on `abc`) returns git's own
  stderr and the file is untouched; ferrit adds no regex of its own.

## App wiring

- `src/app/git_config.rs` (new, state and keys, not drawing): the open flag, the
  cached `ConfigView`, the filter text, the selection, the write scope, the
  pending edit. `App::open_git_config`, `git_config_key`, `apply_config_edit`.
- **Full-screen mode**: the `FullScreen` enum from `PLAN_13_DASHBOARD.md`
  (`FullScreen::GitConfig`). Whichever of phases 13 and 14 lands first adds the
  enum; the second adds its variant.
- **Editing** reuses the text popup (`Popup::Name`-style, `TextInput`,
  `src/components/ui/text_input.rs`): the popup title names the key and the
  target scope. Booleans and enums do not open a popup: `Space` flips a boolean,
  `Enter` opens the same menu widget as the `m` menu (`src/app/menu.rs`) listing
  the allowed values.
- **Confirmations** use the key-bar question (`pending_confirm`), confirmed by
  `y` or `Enter`: `unset` always asks; a write to the **global** file asks once
  per session ("write to ~/.gitconfig?"), naming the file; local writes do not
  ask.
- The worker is not needed: `git config` is local and instant, so writes are
  synchronous like stash and branch actions.

## Secrets and unusual entries

- Values of keys whose name contains `password`, `token`, `secret` or
  `credential` (other than `credential.helper`, which names a program), and URLs
  with an embedded password, are shown redacted with the existing
  `command_log::redact`. Editing such a value replaces it without ever showing
  the old one; the command log records the redacted argument only.
- `include.path` and `includeIf.*` are listed and **read-only** ("edit that file
  directly"). A value that comes from an included file is marked with its origin
  and shown as inherited; changing it writes an override in the chosen scope's own
  file and says so in the message. ferrit never edits an included file.
- The **system** scope is listed and never writable; the hint reads "system,
  read-only".
- **Command-line and environment** entries (`GIT_CONFIG_COUNT`, `-c`) are shown
  as scope `command`, read-only.

## Rendering: `src/app/screens/git_config.rs`

- Columns: scope letter (`S` system, `G` global, `L` local, `W` worktree), key,
  value; section headers (`── core ──`) group by the part before the first dot.
  For `url.<base>.insteadOf` and `alias.<name>` the middle part stays with the
  key.
- The effective value has a `←` marker with the reason ("wins (local)"); a
  shadowed row is dimmed. Colours: `Palette.add` for a local override,
  `warn` for a value that shadows another, default otherwise.
- The header shows the write scope (`[L]ocal` or `[G]lobal`, `s` toggles), the
  key count and the filter. Long values are cut with `…`; `Enter` on a cut value
  opens the edit popup with the full text.

## Keybindings (new)

| Key | Where | Action |
| --- | --- | --- |
| `C` | global | open the git config screen (rebindable, `Action::GitConfig`) |
| `Esc`, `q`, `C` | screen | close it |
| `j` `k` `PgUp` `PgDn` `Home` `End` | screen | move |
| `/` | screen | filter by key or value (`Esc` clears it) |
| `e`, `Enter` | screen | edit the selected key (popup, toggle or picker by type) |
| `Space` | screen | flip a boolean key |
| `a` | screen | add a key (asks for the key, then the value) |
| `d` | screen | unset the key in the write scope (asks) |
| `s` | screen | switch the write scope, local or global |
| `r` | screen | re-read |

`C` is lazygit's copy (cherry-pick) key; ferrit has no cherry-pick yet, and the
key is in the keymap so it can move. The key bar shows `Edit: e | Add: a | Unset: d |
Scope: s | Filter: / | Back: esc` (`Bar::GitConfig`).

## Edge cases

| Case | Behaviour |
| --- | --- |
| not in a repository (bare, or no `.git`) | only the global scope is offered; the local column is hidden |
| `~/.gitconfig` missing | the first global write creates it, after the global confirmation |
| the file is read-only or locked (`config.lock` exists) | git's message is shown; nothing changes |
| a key set at several levels | all values listed; unset removes the write scope's value only, and the message says which value now wins |
| multi-valued key (`remote.origin.fetch`, `credential.helper`) | one row per value; edit changes that value, `a` adds one, `d` removes that value only |
| invalid value | git's stderr in a note; file untouched |
| `includeIf` condition not matching here | its entries are absent from the list, as for git |
| the key being edited changes under it (edited elsewhere) | the write happens, then the re-read shows the real state |
| identity keys | edited here or in the profile drawer; both end in the same `set`, and the drawer's choice is refreshed after |
| very long list (300+ keys) | filter and scroll; no truncation |

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/git_config.rs`: temp repositories with `GIT_CONFIG_GLOBAL` and
  `GIT_CONFIG_SYSTEM=/dev/null` pointing at temp files, so no test touches the
  real `~/.gitconfig`. Cases: parse a listing with every scope and an include,
  `effective` and `shadowed`, set / add / unset at each writable scope, a
  multi-valued key, a typed value that git rejects, a locked file, the redaction,
  and that no call can name the system scope (a compile-level property, checked
  by the absence of a `System` variant in `WriteScope`).
- `test/scripts/160-git-config.script` (the replay harness already isolates git
  config for its fixtures): open `C`, filter, toggle a boolean, edit a string,
  unset with the confirmation, and check the file with `git config` after each.
- A unit test that `KNOWN_KEYS` lists only real git keys (each is accepted by
  `git config --type=` in CI's git).

## Milestones

- **G0** ✅ `config.rs` read: parser, `ConfigView`, tests on captured output (`tests/git_config.rs`).
- **G1** ✅ writes: `set` / `add` / `unset`, `WriteScope`, typed values, tests.
- **G2** `KNOWN_KEYS` and validation through git.
- **G3** `git_config.rs` state, edit popup, toggle and picker, confirmations.
- **G4** `screens/git_config.rs`, redaction, shadowed rows, three widths.
- **G5** keymap entry, key bar, help, route the identity choice through `set`,
  replay script, README row, CHANGELOG line.

## Definition of done (phase 14)

On a real repository a user can: see every key and where it comes from, change
`pull.rebase` locally, flip `commit.gpgsign` globally after one confirmation, add
an alias, and unset a key, with the result identical to running the same
`git config` command. No test or run touches a real config file. A write to the
system scope or to an included file is impossible by construction. Redacted values
never appear on screen or in the command log. `cargo clippy --all-targets
--all-features -- -D warnings` and `cargo test` are green, the replay script
passes, and the README table marks the row ✅.

## Out of scope

- ferrit's own `config.toml` (a later tab of this screen could edit it; it has
  its own schema and comments to preserve, `PLAN_12_POLISH.md` P1)
- the system file, included files, `.git/info/attributes` and `.gitignore`
- editing `.gitmodules`
- a schema-driven form for every git key (the known-keys table is the whole
  typed surface)
- importing or exporting a config
