# Plan: phase 15, create a remote repository from ferrit

**Status: planned.** Nothing here exists yet. lazygit cannot create a remote
repository, so this phase is not compared with it; it is checked with the replay
harness (with a fake `gh`) and screenshots (`PLAN_SELF_TESTING.md`).

## Goal

From a local repository with no remote, create the repository on GitHub, wire it
as `origin` and push the current branch, without leaving ferrit and without
ferrit ever holding a token. It uses the user's own `gh` CLI, already signed in.
It is an outward-facing action (it creates something on someone's account and
uploads their code), so it is explicit at every step, private by default, and
never undoes anything on its own.

```
 P (push) on a repository with no remote                    Step 1: what to do
┌ No remote configured ─────────────────────────────────┐
│  ▸ Create a repository on GitHub                      │
│    Add an existing remote URL                         │
│    Cancel                                             │
└ Enter: choose   Esc: cancel ──────────────────────────┘

 Step 2: details (Tab moves, Enter continues)             Step 3: confirm
┌ Create on GitHub ─────────────────────────────────────┐ ┌ Create richard-lavoura/ferrit ──────┐
│ Owner       richard-lavoura                           │ │ private repository                  │
│ Name        ferrit                                    │ │ then: add remote `origin`, push     │
│ Visibility  ( ) public   (•) private                  │ │ main and set it as upstream         │
│ Description                                           │ │ Enter/y: create   n/Esc: cancel     │
│ Push main after creating   [x]                        │ └─────────────────────────────────────┘
└ Tab: next   Enter: continue   Esc: cancel ────────────┘
 Public asks a second, separate question:
   "Make ferrit PUBLIC? Everyone can read its history. y to confirm"
```

Out, on purpose (see "Out of scope"): GitLab and other hosts, deleting a remote
repository, forks, pull requests, templates.

## The gap this fixes

`P` with no upstream opens an editable `<remote> <branch>` prompt
(`push_current_branch`, `src/app/remote.rs`); with no remote at all it suggests
`origin` and git then fails with "origin does not appear to be a git repository".
Creating the repository means leaving for a browser or `gh repo create`, then
coming back to add the remote by hand. A first push of a new project is exactly
where a terminal Git UI should help.

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

**The remote URL is the user's to confirm.** `gh` writes an `https://` or a
`git@github.com:` URL according to its own setting. A user with several SSH
identities (an alias such as `git@github.com-personal:`) needs a different
host in that URL. After creation ferrit shows the URL it got and lets the user
edit it (the same `Popup::Upstream`-style text input) before the first push; the
edit is `git remote set-url origin <url>`. Nothing is pushed until that step is
confirmed.

## Backend: `src/domain/git/host.rs`

New module, no `ratatui`, next to `remote.rs`. The external program is behind one
small seam so tests never call GitHub:

```rust
pub struct CreateRequest { pub owner: Option<String>, pub name: String,
                           pub visibility: Visibility, pub description: String }
pub enum Visibility { Private, Public }          // `internal` is out of scope
pub struct CreatedRepo { pub url: String, pub web_url: String }

pub(super) fn gh_status() -> GhStatus;            // Missing | SignedOut | Ready { login }
pub(super) fn create_repo(repo: &Repository, req: &CreateRequest, cancel: &AtomicBool)
    -> GitResult<CreatedRepo>;
pub(super) fn set_remote_url(repo: &Repository, name: &str, url: &str) -> GitResult<()>;
```

- `gh_status` runs `gh --version` then `gh auth status`; the login for the owner
  field comes from `gh api user --jq .login`. Organisations are typed by the user
  (`org/name`); ferrit does not list them.
- `create_repo` builds its argument list only from validated fields (below) and
  runs `gh` the way `remote.rs::run_command` runs git: piped output, its own
  process group, the 300-second timeout, cancellable, stdin null.
- The program to run is `gh` from `PATH`, overridable with `FERRIT_GH` so tests
  point at a fake script that records its arguments and prints a canned answer.
  Overriding is a test seam and is documented as such; it is not a config key.
- Every call goes through the command log (`exec::track`) with credentials
  redacted (`command_log::redact`), so the user can see exactly what ran.

**Validation before anything runs** (in ferrit, since `gh`'s errors come late):
the name matches GitHub's rule (letters, digits, `-`, `_`, `.`, not empty, not
`.`/`..`, at most 100 characters); the owner, if given, is a login (letters,
digits, `-`); the description is one line and at most 350 characters. A name that
fails is refused in the popup with the reason, before any process starts.

## App wiring

- `src/app/create_remote.rs` (new): the popup state (step, fields, focus), the
  confirmation, the busy flag, `AppEvent::RemoteCreated`. `App::start_create_remote`,
  `create_remote_key`, `on_remote_created`.
- **Entry points, no new global key.** (1) `P` when `repo.remotes()` is empty opens
  the step-1 choice instead of the bare `origin main` prompt. (2) The `x` menu
  (`src/app/context_menu.rs`) offers "Create a repository on GitHub…" on Status
  and Branches whenever no remote named `origin` exists. With a remote already
  there the entry is hidden.
- **One network operation at a time**: it takes the existing `remote_busy` slot
  (a `RemoteOp::Create` variant) so `f`/`p`/`P` are ignored while it runs and the
  Status pane shows "Creating repository…" like "Pushing…"
  (`remote_busy_label`).
- **Worker**: same thread-plus-event shape as `start_remote_op_with_options`,
  `run_worker(WorkerKind::RemoteOperation, …)`, so a panic still releases the
  slot.
- On success the flow continues without a keypress: `set_remote_url` if the user
  edited the URL, then `push_with_upstream("origin", branch)`; the credential
  popup appears if the SSH key needs a passphrase (`PLAN_9_REMOTE.md`,
  "Credentials"). `RemoteDone` from the push refreshes the panes as today.

## Rendering

The four steps are popups (`Popup::CreateRemote(step)`), drawn by
`src/app/screens/popups.rs` in the same style as the commit and upstream popups
(`draw_commit` for the text fields, the menu widget for step 1). A public
repository shows the public confirmation in the same warning colour the discard
prompt uses. While `gh` runs, the popup is replaced by the status-pane
indicator; nothing modal blocks the UI.

## Safety rules

- **Private by default**, the visibility field starts on private and the summary
  line says the word. Public needs its own second confirmation, separate from the
  general "create?" one, and the default answer is "no".
- **Nothing is created until the last confirmation.** Cancelling at any step
  changes nothing: no remote, no config key, no call to `gh` beyond the read-only
  status checks.
- **No automatic rollback.** If creation succeeds and the push fails (network,
  rejected key), the repository exists and the remote is configured; ferrit says
  so, shows the web URL, and leaves `P` to retry. It never deletes a remote
  repository: that is irreversible, so it is not offered here at all.
- No file is added to the repository: no README, `.gitignore` or licence
  (`gh`'s `--add-readme`, `--gitignore`, `--license` are not passed), because the
  local history is what the user wants published, unchanged.
- Nothing about the user's account is stored: not the login, not the URL beyond
  the remote git already keeps.

## Keybindings

No new global key. In the popups: `Tab` / `Shift-Tab` move between fields, arrows
or `Space` choose visibility, `Enter` continues, `Enter`/`y` confirm the final
question, `Esc`/`n` cancel at any step (the same conventions as the other
popups and key-bar questions).

## Edge cases

| Case | Behaviour |
| --- | --- |
| `gh` not installed | "gh is required: https://cli.github.com" with the option to add a URL instead; nothing else runs |
| `gh` signed out | "run `gh auth login` in a shell, then try again" (ferrit does not run interactive logins) |
| repository name already exists on the account | `gh`'s message shown as is (`Name already exists on this account`), popup stays open on the name field |
| owner is an organisation the user cannot create in | `gh`'s permission message; nothing created |
| a remote called `origin` already exists | the entry is hidden; if `P` reaches this code anyway it says so and does not create |
| detached HEAD | nothing to push: creation is offered, the push step is skipped with a note |
| no commits yet | creation works, the push step is skipped ("commit first") |
| network drops during create | timeout or `gh` error; nothing configured locally (the remote is added only after `gh` reports success) |
| created, push rejected or cancelled | the repository and the remote stay; a note gives the web URL and says `P` retries |
| user cancels the credential popup | the push fails with git's message; same as any push |
| folder name is not a valid repo name (spaces, unicode) | the default is a sanitised version, editable, validated before running |
| running under the replay harness | uses `FERRIT_GH`; without it, the entry is inert (no real `gh` call from tests) |

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/git_host.rs`: a fake `gh` shell script (created in a temp dir, pointed to
  by `FERRIT_GH`) that records its arguments to a file and answers `--version`,
  `auth status`, `api user` and `repo create`. Asserts the exact argument list for
  private, public, organisation and description cases, that `--push`,
  `--add-readme`, `--gitignore` and `--license` are never passed, the name /
  owner / description validation, the timeout and cancel paths, and the
  signed-out and missing-`gh` states.
- `tests/app_create_remote.rs`: `P` with no remote opens the choice; each step's
  keys; public needs the second confirmation and defaults to no; cancelling at
  every step leaves `git remote` empty and the fake `gh` uncalled beyond the
  status reads; success adds `origin` with the URL the fake returned, then pushes
  to a local bare repository set up as that URL.
- `test/scripts/170-create-remote.script`: the whole flow against the fake and a
  local bare repository, ending on the pushed branch and the upstream shown in
  Status.
- The command log test: the create call is recorded with credentials redacted.

## Milestones

- **R0** `host.rs`: `gh_status`, validation, argument building, the fake-`gh`
  seam, tests.
- **R1** `create_repo` and `set_remote_url` with timeout and cancel, tests.
- **R2** `create_remote.rs` state, `RemoteOp::Create`, `AppEvent::RemoteCreated`,
  busy label.
- **R3** the popups (choice, details, confirm, public confirm) and `P` entry.
- **R4** the `x` menu entry, URL edit step, push through `push_with_upstream`.
- **R5** error and edge-case coverage, replay script, README row, CHANGELOG line,
  update `PLAN_9_REMOTE.md` ("no remote" row of its edge cases).

## Definition of done (phase 15)

On a repository with no remote and `gh` signed in, `P` walks to "created and
pushed" in five keypresses plus the name, and `git remote -v` and `git status`
match what `gh repo create` followed by `git push -u origin <branch>` would
give. With `gh` missing or signed out the user gets the exact next command and
nothing changes. No token is read, stored or printed. Public creation cannot
happen by a single keypress. Cancelling anywhere leaves the repository exactly
as it was. `cargo clippy --all-targets --all-features -- -D warnings` and `cargo
test` are green, the replay script passes, and the README table marks the row ✅.

## Out of scope

- GitLab, Bitbucket, Gitea or any host other than GitHub (a later `host.rs`
  trait could take them)
- deleting or archiving a remote repository, changing visibility afterwards
- forks, pull requests, issues, branch protection, secrets, Actions
- `internal` visibility, repository templates, README / `.gitignore` / licence
  initialisation, topics, homepage
- listing the user's organisations (typed as `org/name`)
- signing in to `gh` from ferrit
