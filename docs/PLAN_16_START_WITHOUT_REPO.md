# Plan: phase 16, start ferrit outside a repository

**Status: in progress (W0 to W2 done: the welcome screen works through `App::welcome`; `main.rs` does not use it yet).** lazygit exits with an error
outside a repository, so this phase is not compared with it; it is checked with
the replay harness (a fixture with no repository) and screenshots
(`PLAN_SELF_TESTING.md`).

## Goal

Run `ferrit` in a folder that is not a git repository and get the interface
instead of an error line: a welcome screen that says so and offers `git init`.
After it, the usual panes open on the empty repository, and the user can commit
and, if they want, publish it with phase 15. Nothing is ever initialised without
a question that names the folder.

```
 ferrit, in a folder with no repository above it
┌ ferrit ───────────────────────────────────────────────┐
│                                                       │
│   /Users/me/projects/new                              │
│   is not a git repository.                            │
│                                                       │
│   ▸ i   Initialise a repository here (git init)       │
│     q   Quit                                          │
│                                                       │
└───────────────────────────────────────────────────────┘
 Init: i | Quit: q
        │ i
        ▼
 run git init in /Users/me/projects/new?   Enter/y yes  n/Esc cancel
        │ y
        ▼
 the five panes, an empty repository (no commits)
        │ x           (optional, any time)
        ▼
 Create a repository on GitHub          (phase 15)
```

Out, on purpose (see "Out of scope"): choosing the branch name, adding a
`.gitignore` or a first commit, `git clone`, and `ferrit --path` outside a
repository (it keeps its error).

## The gap this fixes

`App::open` fails with `GitError::NotARepository` and `main.rs` prints
`ferrit: not a git repository: .` and exits 1, before the terminal is taken.
Starting a project then means leaving for `git init`, then starting ferrit. The
phase-15 action that publishes a repository is only reachable once a repository
exists, so the first steps of a new project are exactly where the tool is not
there.

## Approach

**Two ways in, one rule.** `ferrit` with no path argument, in a folder with no
repository in it or above it, opens the welcome screen. `ferrit --path <dir>`
names a folder on purpose, so a non-repository there keeps today's one-line error
and non-zero exit: it is the contract scripts rely on, and a typo in a path must
not offer to create a repository somewhere else. A folder inside a repository is
not "empty": `git2` finds the repository above it, as today.

**An `App` with no repository already exists** (`App::mock`, the render tests):
`repo` is an `Option`, and the actions begin with `let Some(repo) = … else
return`. The welcome screen is one more `FullScreen` variant over that state;
the panes are never drawn, so none of their data is needed.

**After `git init`, the app is rebuilt, not patched.** The identity, the profile
and the first snapshot are all computed from the repository in `App::base`; the
simplest correct move is to open a fresh `App` on the folder (`App::open_with`,
with the config already loaded) and put it in place, carrying over what only
`main` and `run` had set: the graphics probe (`picker`) and the event sender. The
filesystem watcher, which `run` created with no root, is pointed at the new
worktree (`Events::watch`), so a change from another shell is noticed from the
first moment.

## Backend: `src/domain/git/init.rs`

```rust
impl Repo {
    /// `git init` in `dir`, then open it.
    pub fn init(dir: &Path) -> GitResult<Self>;
}
```

- It goes through `exec` like every git command (`git -C <dir> init`), so the
  command log shows it. No flag: the default branch is whatever the user's
  `init.defaultBranch` says, as for `git init` in a shell.
- A failure (a folder that cannot be written, a missing folder) is
  `GitError::InitFailed` with git's own message; nothing is partly created.
- It never runs on an existing repository: the welcome screen only exists when
  there is none, and the rebuilt app is opened right after.

## App wiring

- `App::welcome(dir, load)` builds the no-repository app on the welcome screen:
  `base(None, config)`, `FullScreen::Welcome`, the folder as an absolute path.
  Config problems are reported like `open_with` does.
- `src/app/welcome.rs` (new): the screen's state and keys. `i` asks the question
  (`ConfirmAction::InitRepo`, the key-bar confirm: `Enter` / `y` yes, `n` / `Esc`
  no), `q` and `Esc` quit (exit code 0: the user chose it), nothing else does
  anything. Mouse and the other keys are ignored.
- The question names the absolute folder. When the folder is the user's home
  directory it says so in the same line, because `git init` there is the
  mistake this question exists to catch.
- `App::attach_repository(path)` (also usable on its own, by tests): opens the
  repository at `path`, rebuilds the app as described, and asks `run` to watch
  the new root.
- `Events::watch(root)` replaces the watcher; a failure is the same non-fatal
  "polling fallback" as at startup.
- `main.rs`: `path` becomes `Option<PathBuf>`; `None` means "here, and offer
  `git init`", `Some` means "here, or fail". The error branch and exit code for
  the explicit case do not change.

## Rendering

`screens/welcome.rs`: a `Dialog` centred on the screen, with the folder (cut in
the middle when it is longer than the dialog), the sentence, and the two rows;
the key bar shows `Init: i | Quit: q` (`Bar::Welcome`, fixed like the dashboard's
and the git config screen's). While the question is up the key bar shows it, as
for every other confirm.

## Edge cases

| Case | Behaviour |
| --- | --- |
| a folder inside a repository | opens that repository, no welcome screen (as today) |
| `ferrit --path <dir>` with no repository | the one-line error and exit 1, unchanged |
| the folder is `$HOME` | the question says "this is your home folder" |
| `git init` fails (read-only folder) | git's message in a toast; the welcome screen stays |
| `n` or `Esc` at the question | nothing is created; the welcome screen stays |
| `git` is not installed | the `git init` error is the toast; the screen stays |
| very small terminal | the dialog is clamped to the screen, no panic |
| a change in the folder from another shell before `i` | nothing is watched yet; the poll refresh does nothing without a repository |
| `i` pressed, repository created, the watcher cannot start | the polling fallback and the same note as at startup |
| `Ctrl-C` | quits, as everywhere |

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/git_init.rs`: `Repo::init` in a temp directory makes a repository that
  opens and has no commits, goes through the command log, and fails with git's
  message on a folder that does not exist.
- `tests/app_welcome.rs`: `App::welcome` starts on the welcome screen with the
  folder; `q` and `Esc` quit; `i` asks, naming the absolute folder (and the home
  warning for `$HOME`); `n` / `Esc` create nothing; `y` and `Enter` create the
  repository and leave the screen for the panes; a failing init stays on the
  screen with git's message; the other keys and the mouse do nothing.
  `attach_repository` on an `App::mock` gives a working app.
- `tests/welcome_screen.rs`: the dialog at 120, 80 and 40 columns, the long folder
  cut in the middle, the question in the key bar, tiny terminals.
- `tests/cli.rs`: the binary with `--path` on a folder that is not a repository
  prints the one-line error and exits non-zero (no terminal needed).
- `test/scripts/180-welcome.script`: a fixture with no repository, `i`, `n`, `i`,
  `y`, then the panes and `git rev-parse --is-inside-work-tree => "true"`.

## Milestones

- **W0** ✅ `Repo::init`, `GitError::InitFailed`, tests (`tests/git_init.rs`).
- **W1** ✅ `App::attach_repository` and `Events::watch`, tests on an `App::mock`
  (`tests/app_attach.rs`).
- **W2** ✅ `App::welcome`, the screen's state and keys, the `git init` question,
  the rendering and the key bar (`tests/app_welcome.rs`, `tests/welcome_screen.rs`).
- **W3** `main.rs` (`Option` path, the explicit-path rule), the CLI test, the
  replay fixture and script.
- **W4** README row, CHANGELOG line, plans.

## Definition of done (phase 16)

`ferrit` in a folder with no repository opens the welcome screen; `i`, a yes to a
question that names the folder, and the panes open on an empty repository that
`git status` agrees is one; `ferrit --path` on the same folder still fails with
today's message. Nothing is created without the yes, and a folder that is the
home directory is called out. `cargo clippy --all-targets --all-features -- -D
warnings` and `cargo test` are green, the replay script passes, and the README
table marks the row ✅.

## Out of scope

- choosing the first branch name, a `.gitignore`, a licence or a first commit
- `git clone` (an own phase if ever wanted)
- a welcome screen for `ferrit --path`
- recent folders, or a picker for another folder
- initialising a bare repository
