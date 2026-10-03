# Plan: phase 15, create a remote repository from ferrit

**Status: built (R0 to R5).** `G`, or `x` then `g`, with no remote. lazygit cannot create a remote
repository, so this phase is not compared with it; it is checked with the replay
harness (with a fake `gh`) and screenshots (`PLAN_SELF_TESTING.md`).

## Goal

From a local repository with no remote, create the repository on GitHub, wire it
as `origin` and, if asked, push the current branch, without leaving ferrit and
without ferrit ever holding a token. It uses the user's own `gh` CLI, already
signed in. It is an outward-facing action (it creates something on someone's
account and uploads their code), so it is explicit, private by default, and
never undoes anything on its own.

**It is optional and can happen at any time.** A repository may live locally for
as long as the user likes; nothing nags them to publish it. The action sits in
the `x` menu, and has a key of its own, `G` (as in GitHub), from any pane; both are
there whenever the repository has no remote.

```
 x (Status or Branches), the repository has no remote
┌ Actions ──────────────────────────────┐
│  ▸ Create a repository on GitHub      │      checking `gh`…  (a worker, the
└───────────────────────────────────────┘      form opens on its answer)
        │ Enter
        ▼
┌ Create on GitHub ─────────────────────┐      Three choices, nothing else: the
│╭ Name ──────────────────────── 6/100 ╮│      name (`org/name` for an
││ferrit                               ││      organisation), the visibility and
│╰─────────────────────────────────────╯│      the description. The text boxes
│ Visibility  ( ) public   (•) private  │      wrap over their rows like a commit
│╭ Description ───────────────── 9/350 ╮│      body, with a counter, so what was
││a Rust git UI                        ││      typed stays in view.
││                                     ││
│╰─────────────────────────────────────╯│      `gh` missing or signed out: the
└ Tab: next   Enter: continue   Esc ────┘      popup says what to run instead.
        │ Enter
        ▼
┌ Create richard-lavoura/ferrit ────────┐      public: the word PUBLIC in the
│ PRIVATE repository                    │      discard prompt's warning colour,
│ first: commit README.md, by Ferrit    │      and only `y` confirms; Enter
│ then: add remote `origin` over ssh    │      cancels. The last two lines are
│   host github.com-personal, push main │      not choices.
│ Enter/y: create        n/Esc: cancel  │
└───────────────────────────────────────┘
        │ Enter/y        ← last way back: nothing exists yet
        ▼
 gh repo create <owner/name> --private --source <dir> --remote origin
   → the repository exists, `origin` is added
        ▼
 git push -u origin <branch>     (by ferrit, always)
   → the passphrase popup shows if the SSH key needs one
```

Out, on purpose (see "Out of scope"): GitLab and other hosts, deleting a remote
repository, forks, pull requests, templates, editing the remote URL, starting
ferrit outside a repository.

## The gap this fixes

`P` with no upstream opens an editable `<remote> <branch>` prompt
(`push_current_branch`, `src/app/remote.rs`); with no remote at all it suggests
`origin` and git then fails with "origin does not appear to be a git repository".
Creating the repository means leaving for a browser or `gh repo create`, then
coming back to add the remote by hand. A first push of a new project is exactly
where a terminal Git UI should help. `P` itself does not change: this phase adds
one explicit action and leaves the push keys alone.

## Approach

**Use `gh`, not the GitHub API.** ferrit stores no credential, parses no token,
and needs no OAuth flow: `gh` is already authenticated, handles SSO and
organisations, and writes the remote. If `gh` is absent or signed out, ferrit
says exactly what to run (`gh auth login`) and stops; it does not try to sign in
for the user.

**Create, then push in two separate steps.** `gh repo create … --push` would push
through `gh`'s own git call, bypassing ferrit's credential popup
(`domain/git/askpass.rs`) and the user's SSH configuration. So ferrit runs
`gh repo create <owner/name> --private|--public --source <workdir> --remote origin
[--description …]` **without** `--push`, then pushes with its own existing path
(`push_with_upstream`, `git push -u origin <branch>`), which already handles
passphrases and SSH host aliases.

**The URL carries the user's SSH host.** `gh` writes an `https://` or a
`git@github.com:` URL according to its own setting. With several accounts on one
machine, the key of the right one is declared for a host alias in
`~/.ssh/config` (`Host github.com-personal`, `HostName github.com`,
`IdentityFile ~/.ssh/github-personal`). A remote written with the plain
`github.com` offers none of those keys: `ssh` finds no key to unlock and the push
fails before any passphrase is asked, so ferrit's passphrase popup never shows,
unlike for the user's other repositories whose `origin` uses the alias. So the
user's alias is used **automatically**, with no field for it: the first GitHub
alias of `~/.ssh/config` (read, never written; none when there is none), named on
the last question (`using your SSH key for github.com-personal`). After `gh` creates the
repository, ferrit rewrites `origin` to
`git@<host>:<owner>/<name>.git` (owner and name from the web URL `gh` printed)
with `git remote set-url`, and only then pushes, through the same path as `P`, so
the key is the one the alias names and the popup answers for it. With no alias,
`origin` keeps the URL `gh` wrote. If the rewrite fails, `origin` keeps `gh`'s URL, the
note says so and how to fix it, and nothing is pushed. Editing the URL of an
existing remote later is not part of this phase.

## Backend: `src/domain/git/host.rs`

New module, no `ratatui`, next to `remote.rs`. The external program is behind one
small seam so tests never call GitHub:

```rust
pub struct CreateRequest { pub owner: Option<String>, pub name: String,
                           pub visibility: Visibility, pub description: String }
pub enum Visibility { Private, Public }          // `internal` is out of scope
pub struct CreatedRepo { pub web_url: String }
pub enum GhStatus { Missing, SignedOut, Ready }

pub fn parse_target(text: &str) -> Result<(Option<String>, String), NameError>;
pub fn gh_status(gh: &GhProgram) -> GhStatus;
pub(super) fn create_repo(repo: &Repository, gh: &GhProgram, req: &CreateRequest,
                          cancel: &AtomicBool) -> GitResult<CreatedRepo>;
```

- `gh_status` runs `gh --version`, then `gh auth status`. There is no owner field
  and no `gh api user`: the owner is whatever the user typed (`org/name`) or, left
  out, `gh`'s own default (the signed-in account).
- `create_repo` builds its argument list only from validated fields (below) and
  runs `gh` the way `remote.rs` runs git: piped output, its own process group,
  the 300-second timeout, cancellable, stdin null.
- **The program is injected, not read from the environment.** `GhProgram` is the
  program to run, `gh` from `PATH` by default, with the timeout (`with_timeout`
  lets a test make it short). `App` holds it (`App::set_gh_program`, a
  `#[doc(hidden)]` seam) and hands it to the worker, which opens its own `Repo`
  and so could not carry it; a test points it at a fake script that records its
  arguments and prints a canned answer, in the spirit of `Repo::isolate_config`
  (`PLAN_14_GIT_CONFIG.md`). The crate forbids `unsafe`, so a test cannot set an
  environment variable in-process; the replay runner sets the program the same
  way. It is a seam, not a config key.
- **One place builds an external command.** `exec.rs` gains `exec::program`
  beside `exec::git`, and `Tracked` writes the program's own name in the log
  (`gh repo create …`, not `git …`). `tests/git_exec.rs` keeps scanning the
  sources so nothing else builds a `Command`.
- Every call goes through the command log (`exec::track`) with credentials
  redacted (`command_log::redact`), so the user can see exactly what ran.

**Validation before anything runs** (in ferrit, since `gh`'s errors come late):
the name matches GitHub's rule (letters, digits, `-`, `_`, `.`, not empty, not
`.`/`..`, at most 100 characters); an owner, typed as the part before the `/`, is a
login (letters, digits, `-`); the description is one line and at most 350
characters. A target that fails is refused in the popup with the reason, before
any process starts.

## App wiring

- `src/app/create_remote.rs` (new): the popup state (step, fields, focus), the
  confirmation, the busy flag, `AppEvent::RemoteCreated`. `App::start_create_remote`,
  `create_remote_key`, `on_remote_created`.
- **Entry points: `G`, and the `x` menu; no change to `P`.** `G` is the global
  action `create_remote` (rebindable, `[keys.global]`): from any pane it starts the
  flow, and with a remote already there it says so in a note and asks `gh`
  nothing. The `x` menu (`src/app/context_menu.rs`) also offers "Create a
  repository on GitHub" on Status and Branches whenever the repository has **no
  remote at all**, so the action can be found without knowing the key; with any
  remote already there (named `origin` or not) that entry is hidden.
- **The `gh` check is off the UI thread.** Choosing the entry starts a worker that
  runs `gh_status` (it reaches the network); the popup reads "checking gh…" and
  turns into the form on `Ready`, or into the message for `Missing` / `SignedOut`.
- **The draft survives the operation.** While `gh` runs the popup is replaced by
  the status-pane indicator, but the typed fields stay on `App`, so a refusal
  (the name is taken, no right to create there) reopens the form on the name
  field with the text kept, and nothing has to be retyped.
- **One network operation at a time**: it takes the existing `remote_busy` slot
  (a `RemoteOp::Create` variant) so `f`/`p`/`P` are ignored while it runs and the
  Status pane shows "Creating repository…" like "Pushing…"
  (`remote_busy_label`).
- **Worker**: same thread-plus-event shape as `start_remote_op_with_options`,
  `run_worker(WorkerKind::RemoteOperation, …)`, so a panic still releases the
  slot.
- On success the flow continues without a keypress, always:
  `push_with_upstream("origin", branch)`; the credential popup appears if the SSH
  key needs a passphrase (`PLAN_9_REMOTE.md`, "Credentials"). `RemoteDone` from the
  push refreshes the panes as today. A detached `HEAD` has nothing to push: a note
  says so and `origin` stays set.

## Rendering

The popups (`Popup::CreateRemote(step)`: checking, form, confirm) are drawn by
`src/app/screens/popups.rs` in the same style as the commit and upstream popups
(`draw_commit` for the text fields). A public repository shows the confirmation
in the same warning colour the discard prompt uses. While `gh` runs, the popup is
replaced by the status-pane indicator; nothing modal blocks the UI.

## Safety rules

- **Private by default**, the visibility field starts on private and the summary
  line says the word. Public is confirmed by `y` alone: **Enter does not confirm
  it**, unlike every other key-bar question of ferrit, and the default answer is
  "no". That is the one place the "Enter confirms" convention (CHANGELOG 0.8.0)
  is deliberately broken, and it has its own test.
- **Nothing is created until the last confirmation.** Cancelling at any step
  changes nothing: no remote, no config key, no call to `gh` beyond the read-only
  status checks.
- **No automatic rollback.** If creation succeeds and the push fails (network,
  rejected key), the repository exists and the remote is configured; ferrit says
  so, shows the web URL, and leaves `P` to retry. It never deletes a remote
  repository: that is irreversible, so it is not offered here at all.
- No file is added to the repository, with one exception that is not a choice: the
  **initial commit**. `gh`'s `--add-readme`, `--gitignore` and `--license` are never
  passed, and no `.gitignore` or licence is made. When the repository has **no
  commit at all**, ferrit always makes a first commit holding an empty
  `README.md` (an existing one is committed as it is, never overwritten), so a
  new project is published and pushed with nothing else to set up; the last
  question announces it. The message is always the same, and says who made what:
  `Initial commit`, then `This initial commit and the remote repository were
  created by Ferrit.` Only that file goes in: other files of the folder stay
  untracked, and anything the user staged stays staged. A repository that already
  has a commit gets none, and asking twice commits once. The commit is local and
  made **before** anything is created: if it fails (a hook, no identity), nothing
  exists anywhere and the form reopens with git's message. If the creation is then
  refused, the commit stays and a retry does not make a second one. It is `git
  commit`, so hooks, signing and the identity are the user's own, and the author
  chosen in the profile drawer applies.
- Nothing about the user's account is stored: not the login, not the URL beyond
  the remote git already keeps.

## Keybindings

One global key, `G` (`Action::CreateRemote`). In the popups: `Tab` / `Shift-Tab` move between fields, arrows
or `Space` choose visibility, `Enter` continues, `Enter`/`y` confirm a private
creation and `y` alone a public one, `Esc` closes the form, `n`/`Esc` at the last
question go back to the form with its fields (nothing was created), and `Esc`
closes at any other step (the same
conventions as the other popups and key-bar questions, but for the public case).

## Edge cases

| Case | Behaviour |
| --- | --- |
| `gh` not installed | "gh is required: https://cli.github.com"; nothing else runs and no form shows |
| `gh` signed out | "run `gh auth login` in a shell, then try again" (ferrit does not run interactive logins) |
| repository name already exists on the account | `gh`'s message shown as is (`Name already exists on this account`), the form reopens on the name field with its text kept |
| owner is an organisation the user cannot create in | `gh`'s permission message; nothing created; the form reopens |
| the repository has a remote (any name) | the menu entry is hidden; if the action is reached anyway it says so and does not create |
| detached HEAD | nothing to push: creation is offered, the push step is skipped with a note |
| no commits yet | the first commit is made first, always (an empty `README.md`, the same message), so there is something to push |
| the first commit fails (a `pre-commit` hook, no identity) | nothing is created; the form reopens on the name with git's message |
| network drops during create | timeout or `gh` error; nothing configured locally (`gh` adds the remote only once it succeeds) |
| created, push rejected or cancelled | the repository and the remote stay; a note gives the web URL and says `P` retries |
| user cancels the credential popup | the push fails with git's message; same as any push |
| the URL `gh` wrote does not reach the user's key (several accounts, a host alias) | `origin` is rewritten over the first GitHub alias of `~/.ssh/config` before the push; with no alias it keeps `gh`'s URL |
| `gh`'s web URL has no owner and name | `origin` keeps `gh`'s URL, a note says so, nothing is pushed |
| folder name is not a valid repo name (spaces, unicode) | the default is a sanitised version, editable, validated before running |
| running under the replay harness | the runner installs a fake `gh` for every session (`src/replay/fake_gh.rs`): the replay never runs a real one; tests of other features never reach the entry |

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/git_host.rs`: a fake `gh` shell script (created in a temp dir, handed to
  `App::set_gh_program`) that records its arguments to a file and answers
  `--version`, `auth status` and `repo create`. Asserts the exact argument list
  for private, public, organisation and description cases, that `--push`,
  `--add-readme`, `--gitignore` and `--license` are never passed, the name /
  owner / description validation, the timeout and cancel paths, and the
  signed-out and missing-`gh` states.
- `tests/app_create_remote.rs`: `G` opens it from any pane and says so when there is
  a remote, and is a letter inside the form; the `x` entry shows only with no
  remote at all;
  the check runs off the UI thread; each step's keys; public needs `y` and
  **Enter cancels it**; cancelling at every step leaves `git remote` empty and
  the fake `gh` uncalled beyond the status reads; a refusal reopens the form with
  its text; success adds `origin` with the URL the fake wrote, then pushes to a
  local bare repository set up as that URL.
- `test/scripts/170-create-remote.script`: the whole flow against the fake and a
  local bare repository, ending on the pushed branch and the upstream shown in
  Status.
- The command log test: the create call is recorded as `gh …`, credentials
  redacted; `tests/git_exec.rs` still finds no command built outside `exec`.

## Milestones

- **R0** ✅ `host.rs`: types, name / owner / description validation, argument
  building (`tests/git_host.rs`); then `exec::program`, `gh_status` and the
  injected `GhProgram`, tested with a fake `gh` script.
- **R1** ✅ `Repo::create_repo` with timeout and cancel, tests; `gh_status` goes
  through the same timeout (the child-process runner of `remote.rs` is shared).
- **R2** ✅ `create_remote.rs` state, `RemoteOp::Create`, `AppEvent::RemoteCreated`,
  busy label, the draft kept across the operation (`tests/app_create_remote.rs`).
- **R3** ✅ the popups (checking, form, confirm with the Enter-does-not-confirm
  public case) and the `x` menu entry (`tests/create_remote_screen.rs`).
- **R4** ✅ push through `push_with_upstream`, the partial-failure notes.
- **R5** ✅ edge-case coverage, replay script (`src/replay/fake_gh.rs` installs the
  fake for every session), README row, CHANGELOG line, `PLAN_9_REMOTE.md` ("no
  remote" row of its edge cases).

## Definition of done (phase 15)

On a repository with no remote and `gh` signed in, `G` walks to "created and
pushed" in a handful of keypresses plus the name, and `git remote -v` and
`git status` match what `gh repo create` followed by `git push -u origin
<branch>` would give. With `gh` missing or signed out the user gets the exact
next command and nothing changes. No token is read, stored or printed. Public
creation cannot happen by Enter or by a single unconfirmed key. Cancelling anywhere
leaves the repository exactly as it was. `cargo clippy --all-targets
--all-features -- -D warnings` and `cargo test` are green, the replay script
passes, and the README table marks the row ✅.

## Out of scope

- GitLab, Bitbucket, Gitea or any host other than GitHub (a later `host.rs`
  trait could take them)
- deleting or archiving a remote repository, changing visibility afterwards
- forks, pull requests, issues, branch protection, secrets, Actions
- `internal` visibility, repository templates, README / `.gitignore` / licence
  initialisation, topics, homepage
- listing the user's organisations (typed as `org/name`)
- signing in to `gh` from ferrit
- editing the URL of an existing remote from ferrit (the creation sets it once, over the user's SSH alias)
- starting ferrit in a folder that is not a repository (phase 16: a welcome
  screen offering `git init`, `PLAN_16_START_WITHOUT_REPO.md`)
