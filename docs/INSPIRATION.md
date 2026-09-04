# Inspiration and prior art

Projects worth studying before and while building `ferrit`. For each: what to
learn from it, and where we want to differ.

**Local reference checkout:** the code-heavy repos below are collected in the
private [`ferrit-rs/ferrit-references`](https://github.com/ferrit-rs/ferrit-references)
repo as pinned submodules, so maintainers read the same commits and line
numbers. Clone with `git clone --recurse-submodules --shallow-submodules`.
Do not run `cargo build` inside a submodule. Never vendored into `ferrit`;
any reused block goes through `THIRD-PARTY.md`.

## The reference: lazygit

- **lazygit** — <https://github.com/jesseduffield/lazygit> (Go, gocui)
  The UX target. Panel layout, context-based keybindings, the "everything is a
  list you act on" model, interactive rebase UI, custom commands, undo via
  reflog. Read `pkg/gui` for the panel/context system.
  We differ: native Rust, no Go runtime; stricter separation between git
  backend and UI; aim for a smaller, more predictable keymap.

## Rust git TUIs (direct prior art)

- **gitui** — <https://github.com/gitui-org/gitui> (Rust, ratatui)
  Closest existing thing. Async git operations off the UI thread, syntax
  highlighting, hunk staging, blame, stashing. Study `asyncgit/` (worker
  threads + progress) and how it keeps the UI responsive on big repos.
  We differ: lazygit-style workflow and keymap rather than gitui's own; more
  emphasis on branch/rebase flows.

- **gitu** — <https://github.com/altsem/gitu> (Rust, ratatui)
  Magit-inspired. File / hunk / line level staging, amend, fixup, autosquash,
  interactive rebase, transient-style menus. Good example of a small codebase
  that still covers advanced rebase flows.

- **gex** — <https://github.com/Piturnah/gex> (Rust)
  Magit-style staging porcelain, minimal. Useful as a "smallest thing that
  works" reference for the status + stage/unstage loop.

- **gitrs** — <https://github.com/qleveque/gitrs> (Rust, ratatui)
  `tig`-inspired, fast log browsing. Note: crate name `gitrs` is taken by an
  unrelated tool; this is just the repo.

- **serie** — <https://github.com/lusingander/serie> (Rust, ratatui)
  Rich commit-graph rendering in the terminal. Reference for drawing the
  branch/merge graph well (Unicode, colours, lane assignment).

- **git-graph** — <https://github.com/mlange-42/git-graph> (Rust)
  Commit graph layout algorithm, configurable branching models. Not a TUI but
  the graph logic is reusable.

## lazygit rewrites in Rust (hobby / small)

Both MIT, so code is legally reusable if we keep their copyright notice for any
non-trivial copied block (record it in `THIRD-PARTY.md`). Neither is an
architecture reference; read them for feature ideas.

- **lazygitrs** — <https://github.com/Blankeos/lazygitrs> (Rust, MIT)
  "For me" fork of lazygit. Feature menu worth cherry-picking: AI commit
  message generation (multi-LLM), 30+ themes with live switching, side-by-side
  syntax-highlighted diff, GitHub conveniences (copy repo URL, PR ops),
  editor integration (Helix, Neovim). Author calls it a slopfork; ideas yes,
  code no.

- **rustygit** — <https://github.com/rustyorg/rustygit> (Rust, MIT)
  git TUI inspired by lazygit. Tiny, ~28 commits, unmaintained. Nothing to
  take. Note: unrelated to the `rustygit` *library* crate by Keir Lawson.

## Wider ratatui + git ecosystem

Small single-purpose tools. Each solves one slice of what ferrit needs; good
for seeing "how did they model this one screen".

- **gitu** — <https://github.com/altsem/gitu> — magit-like client (also listed above).
- **serie** — <https://github.com/lusingander/serie> — rich commit graph (also above).
- **giff** — <https://github.com/bahdotsh/giff> — git diff TUI with interactive
  rebase support. Compare its rebase UI with gitu's.
- **git-time-machine** — <https://github.com/dinakars777/git-time-machine> —
  visual reflog TUI for undoing mistakes. Reference for an "undo" view.
- **deadbranch** — <https://github.com/armgabrielyan/deadbranch> — safely clean
  stale branches. Reference for the branch-list + bulk-action pattern.
- **Gitside** — <https://github.com/dev-bhaskar8/gitside> — responsive,
  mouse-friendly SCM TUI that adapts to narrow tmux panes. Reference for
  responsive layout and mouse support.
- **gwm** — <https://github.com/kbrdn1/gwm-cli> — git worktree manager, CLI and
  TUI in one binary, native libgit2. Reference for worktree handling and for
  shipping CLI + TUI from one binary.
- **blippy** — <https://github.com/AksharP5/blippy> — keyboard-first TUI for
  GitHub issues and PRs. Relevant if ferrit grows a PR/review panel.
- **gmsg** — <https://github.com/olorikendrick/gmsg> — generate / edit / commit
  AI commit messages from one TUI. Second data point for the AI-commit feature.
- **gimoji** — <https://github.com/zeenix/gimoji> — emoji picker for commit
  messages. Tiny; useful widget pattern for a searchable picker.
- **wrkflw** — <https://github.com/bahdotsh/wrkflw> — validate and run GitHub
  Actions workflows locally. Out of scope now, note for a future CI panel.
- **repgrep** — <https://github.com/acheronfail/repgrep> — interactive
  find/replace across files on top of ripgrep. Not git, but a clean model for
  an interactive multi-file action list.

Keep scanning **awesome-ratatui** (<https://github.com/ratatui/awesome-ratatui>,
"Git / version control" section) as new ones appear.

## Adjacent VCS TUIs

- **lazyjj** — <https://github.com/Cretezy/lazyjj> (Rust, ratatui)
  lazygit-style client for Jujutsu. Same shape as ferrit, different backend.
  Good layout and component reference.

- **jj / jujutsu** — <https://github.com/jj-vcs/jj> (Rust)
  Git-compatible VCS. Worth reading for ideas on operation log / undo model
  and how they present conflicts.

- **radicle-tui** — <https://github.com/radicle-dev/radicle-tui> (Rust, ratatui)
  Patch and issue review TUI. Component architecture and state management
  patterns on top of ratatui.

## Classic (non-Rust) git UIs

- **tig** — <https://github.com/jonas/tig> (C, ncurses)
  The original git TUI. Views model (main / diff / log / blame / refs / stash),
  and how you drill between them. Keybinding reference.

- **magit** — <https://github.com/magit/magit> (Emacs Lisp)
  Gold standard git UX. The "status buffer as command center", transient
  menus, section folding. Even if we do not copy it, know it.

- **GitButler** — <https://github.com/gitbutlerapp/gitbutler> (Rust core, non-TUI)
  Virtual branches, applying work to multiple branches at once. Ideas for
  branch/workflow model, not for UI.

## Diff and rendering

- **delta** — <https://github.com/dandavison/delta> (Rust)
  Syntax-highlighted, side-by-side diffs; word-level diff highlighting. Study
  how it themes and lays out diffs; we want similar quality inside the TUI.

- **difftastic** — <https://github.com/Wilfred/difftastic> (Rust)
  Structural (AST) diff. Stretch goal for a "smart diff" mode.

- **syntect** — <https://github.com/trishume/syntect> (Rust)
  Syntax highlighting engine delta uses. Candidate for our diff view.

## Git backend options

- **git2-rs** — <https://github.com/rust-lang/git2-rs> (libgit2 bindings)
  Pragmatic default. Mature, covers almost everything. C dependency.

- **gitoxide / gix** — <https://github.com/GitoxideLabs/gitoxide> (pure Rust)
  Pure-Rust git implementation, very fast for reads. Coverage of write/rebase
  operations is still growing. Possible long-term backend or partial use
  (fast log/status via gix, mutations via git2 or shelling out).

- **shelling out to `git`** — always an option for operations neither library
  covers cleanly (interactive rebase, some merge cases). lazygit does this a
  lot.

## TUI stack (ratatui ecosystem)

- **ratatui** — <https://github.com/ratatui/ratatui> — the TUI crate.
- **awesome-ratatui** — <https://github.com/ratatui/awesome-ratatui> — catalogue
  of apps and widgets; scan it before writing a widget from scratch.
- **crossterm** — <https://github.com/crossterm-rs/crossterm> — terminal backend.
- **tui-textarea** — <https://github.com/rhysd/tui-textarea> — multiline text
  editing widget (commit messages, interactive rebase todo editing).
- **tui-input** — <https://github.com/sayanarijit/tui-input> — single-line input.
- **bottom** — <https://github.com/ClementTsang/bottom> (Rust, ratatui) — not
  git, but a well-structured large ratatui app: event loop, layout, data
  refresh. Architecture reference.
- **notify** — <https://github.com/notify-rs/notify> — filesystem watching, to
  refresh state when the repo changes outside ferrit.

## What ferrit should take from all of this

- lazygit's workflow and mental model
- gitui's async-git-off-the-UI-thread discipline
- gitu's compact handling of advanced rebase flows
- serie's commit-graph rendering quality
- delta's diff presentation quality
- a clean split: `git backend crate` <-> `TUI crate`, so the backend is
  testable and reusable without a terminal
