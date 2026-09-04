# Inspiration and prior art

Projects worth studying before and while building `ferrit`. For each: what to
learn from it, and where we want to differ.

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
