# Flow catalogue

Every workflow worth comparing with lazygit (`/compare-lazygit`), by domain. A row
is one flow file in `test/flows/`; keep a flow to 6-10 steps so the run and the
reading of its screenshots stay short. The plan of each domain says which flows to
run when a feature in it changes.

## What to actually write

Not all of it. Write the flows a user runs every day (B1 to B6, C1 to C5, D1 to D4,
D7, F1 to F3, E1 to E4, G2, G3, H1 to H3) and the interface ones (L5 to L11). The
rest waits for the feature it belongs to (see "Which flows to run when"). Never
write the N/A rows or the rare cases (K6, L15) before a bug asks for them.

Every flow has a `focus` and a `not` line. A git flow judges git behaviour and what
the panels say about it; the interface's own behaviour (sizes, focus, clicks, wheel,
launch) is judged once, in a section L flow.

Status:

- **DONE**: the flow exists.
- **READY**: only a flow file, same keys in lazygit and ferrit, no setup.
- **SETUP**: needs a scenario built with `sh` lines (dirty files, a local bare
  remote, a conflict, a hook). See the recipes at the bottom.
- **KEYS**: the two programs use different keys, or ferrit's key is unknown. Needs
  per-program keys in the runner (tooling T1), and the keys read from each
  program's `?` screen first.
- **TOOL**: needs another runner feature (T2 to T4).
- **N/A**: no lazygit counterpart to compare with.

## A. Reading and navigation (PLAN_1, PLAN_3, PLAN_4)

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| A1 | Repo tour | panels 1 to 5, a commit and its files, help | DONE (repo-tour) |
| A2 | Focus and tabs | 1 to 5, Tab and Shift-Tab, Local / Remotes / Tags tabs | READY |
| A3 | Scroll and paging | line, page, half page, top, bottom in Commits and in a long diff (see L8 for the interface side) | KEYS |
| A4 | Commit detail | Enter on a commit, its files, a file's patch, Esc back | READY |
| A5 | Help and command log | the help screen, the command log opened and closed | READY |
| A6 | Search or filter in a panel | lazygit filters with `/`; check whether ferrit has it | KEYS |

## B. Staging and discard (PLAN_6)

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| B1 | Stage all, commit | the whole round trip | DONE (feature-workflow) |
| B2 | Space on a directory | stages everything under it | DONE (stage-directory) |
| B3 | Toggle one file | stage and unstage one file, marker colours change | SETUP |
| B4 | Stage one hunk | open the diff, stage a hunk, the rest stays unstaged | SETUP, KEYS |
| B5 | Stage lines with a range | select a range, stage it | SETUP, KEYS |
| B6 | Discard a file | the confirmation, both answers | SETUP |
| B7 | Discard a hunk or lines | in the diff view | SETUP, KEYS |
| B8 | Partly staged file | markers for a file with staged and unstaged parts, both diffs | SETUP |
| B9 | Deleted, renamed, mode-changed | markers and diff for each | SETUP |
| B10 | New untracked directory | folding, markers | SETUP |
| B11 | Binary and image file | diff pane message, image preview | SETUP |
| B12 | Very large diff | scrolling, speed | SETUP |

## C. Commit (PLAN_7)

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| C1 | Commit with a description | Tab to the description, several lines, confirm | READY |
| C2 | Amend HEAD | pre-filled message, confirm | READY |
| C3 | Reword HEAD | lazygit `r` in Commits, ferrit `w` | KEYS |
| C4 | Nothing staged | the "stage everything?" question, both answers | READY |
| C5 | Empty message | refused, no commit made | READY |
| C6 | Message history | up and down in the popup | READY |
| C7 | Sign-off and no-verify | lazygit's menu, ferrit's inline toggles | KEYS |
| C8 | Failing pre-commit hook | the error shown, nothing committed | SETUP |
| C9 | commit.template | a new commit starts pre-filled | SETUP |
| C10 | Long summary | counter and colour past the limit | READY |

## D. Branches (PLAN_8)

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| D1 | Create a branch, commit on it | the round trip | DONE (feature-workflow) |
| D2 | Checkout an existing branch | selection, what the panels show after | KEYS |
| D3 | Checkout with a dirty tree | a blocking change, the message | SETUP, KEYS |
| D4 | Delete a branch | merged and unmerged, the confirmation | SETUP, KEYS |
| D5 | Rename a branch | prompt, result | KEYS |
| D6 | Fast-forward from upstream | ferrit `u` (Fast-forward), lazygit `u` (upstream menu) | SETUP, KEYS |
| D7 | Merge into the current branch | fast-forward and no-ff, the graph after | SETUP, KEYS |
| D8 | Remotes and Tags tabs | lists, checkout of a remote branch | SETUP |
| D9 | Create and delete a tag | prompt, list | KEYS |
| D10 | Detached HEAD | checkout a commit, how Status says it, back to a branch | READY |
| D11 | Branch from a commit | `n` in Commits | KEYS |

## E. Remote (PLAN_9)

All SETUP: a local bare origin, and a second clone to play "someone else".
E7 is free: the copy's remotes already point nowhere.

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| E1 | Fetch | the behind marker appears after another clone pushed | SETUP |
| E2 | Pull, fast-forward | | SETUP |
| E3 | Pull, diverged | the merge or rebase question | SETUP |
| E4 | Push | a new branch: the upstream question | SETUP |
| E5 | Push rejected, then force | the force prompt | SETUP |
| E6 | Ahead and behind over time | the markers in Status and Branches after each action | SETUP |
| E7 | Remote unreachable | the error message and how long it blocks | READY |

## F. Stash (PLAN_10)

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| F1 | Stash push | the message prompt, the empty tree after | SETUP |
| F2 | Stash with untracked, keep index | the options | SETUP, KEYS |
| F3 | Apply, pop, drop | each result, the list after | SETUP |
| F4 | Stash files view | Enter on a stash, its files and patch | SETUP |
| F5 | Rename a stash | | KEYS |

## G. History rewriting (PLAN_11)

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| G1 | Reword an older commit | | KEYS |
| G2 | Drop a commit | the confirmation, the list after | READY |
| G3 | Squash and fixup | | KEYS |
| G4 | New fixup, autosquash | | SETUP, KEYS |
| G5 | Edit a commit | stops, amend, continue | SETUP |
| G6 | Move a commit up or down | | KEYS |
| G7 | Interactive rebase onto a branch | | SETUP |
| G8 | Rebase with a conflict | continue, abort, skip | SETUP |
| G9 | Reset soft, mixed, hard | the menu, each result | KEYS |
| G10 | Cherry-pick, revert | copy and paste, revert | KEYS |

## H. Conflicts (PLAN_11)

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| H1 | A merge conflict appears | markers, the message, the panels | SETUP |
| H2 | Take ours or theirs | ferrit's take-a-side; lazygit resolves in its merge view | KEYS |
| H3 | Resolve and continue | stage, commit or continue | SETUP |
| H4 | Abort the merge | | SETUP |

## I. Menus, prompts, errors (PLAN_12)

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| I1 | Context menu | `x` in both (in lazygit it lists keybindings) | READY |
| I2 | Operation menu | while a merge or rebase is in progress | SETUP |
| I3 | Confirm and cancel | Esc, n, y behave the same across prompts | READY |
| I4 | A git error surfaced | e.g. a checkout git refuses | SETUP |
| I5 | Configuration effects | theme, mouse, commit settings: the models differ | N/A |

## J. Input and terminal (PLAN_5, PLAN_12)

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| J1 | Mouse | click a panel and a row, wheel, right click: moved to section L, gestures now exist | DONE (L2, L3) |
| J2 | Terminal sizes | 80x24, 120x30, 250x60 | TOOL (T2) |
| J3 | Resize during a session | | TOOL (T2) |

## K. Starting states

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| K1 | Empty repository | no commit yet | SETUP |
| K2 | Only untracked files | | SETUP |
| K3 | Start in the middle of a merge or rebase | | SETUP |
| K4 | Start on a detached HEAD | | SETUP |
| K5 | Not a repository | start outside one: the message | TOOL (T4) |
| K6 | Worktrees and Submodules tabs | | SETUP |
| K7 | Many branches, long names | truncation | SETUP |

## L. Interactions with the TUI itself (no git feature involved)

The user's other half: what the pointer and the keyboard do to the interface,
whatever the repository holds. Gestures are named after panels (`click:commits:4`,
`wheel:commits:down:3`, `scrollbar:commits:50`, `dragbar:commits:0:100`, see
`.dev-tools/tui-mouse.py`). Run on the real repository copy.

| ID | Workflow | What it checks | Status |
|---|---|---|---|
| L1 | Launch | first screen, focus, key bar, panel sizes | DONE (ui-mouse, step 1) |
| L2 | Click a panel, a row, the right pane | focus, selection, right pane, key bar | DONE (ui-mouse) |
| L3 | Wheel over focused and unfocused panels, and the right pane | what scrolls, selection or view | DONE (ui-mouse; step 12 to rerun on a long patch) |
| L4 | Scrollbar click and drag | jump, thumb drag, range by drag | DONE (ui-mouse; the column hit is not confirmed) |
| L5 | Right click on a row | the menu and its entries | READY |
| L6 | Click a key hint in the key bar | runs its action (ferrit has it, PLAN_12) | READY |
| L7 | Double click | lazygit enters on double click; ferrit skips it on purpose (PLAN_5) | READY |
| L8 | Keyboard scroll and cursor in the right pane | line, page, half page, top, bottom | KEYS |
| L9 | Focus by number and Tab | 1 to 5, Tab, Shift-Tab, left and right | READY |
| L10 | Click on a popup, outside it, on its buttons | dismiss, confirm | READY |
| L11 | Click while a prompt is open | is the click ignored, or does it leave the prompt | READY |
| L12 | Resize during a session | the layout follows | TOOL (T2) |
| L13 | Very small terminal | 80x24 and below: what is shown, what breaks | TOOL (T2) |
| L14 | Very wide terminal | the layout on 250 columns | TOOL (T2) |
| L15 | Long lines and wide characters in a panel | truncation, wrapping | SETUP |

## Tooling to add

- **T1, per-program keys**: `step NAME` with a keys line for each program. Needed by
  every KEYS row. Read both programs' `?` screens first: never assume a key.
- **T2, terminal size**: a `size WxH` directive (the Terminal window and the tmux
  session are 200x50 today).
- **T3, mouse**: done. `.dev-tools/tui-mouse.py` sends SGR mouse sequences through
  tmux; both programs receive clicks, wheel and drags. What is not solved: hitting
  the scrollbar column with certainty.
- **T4, start directory**: run a program outside a repository, or with arguments.
- **T5, scenario recipes**: `test/flows/lib/*.sh` (below), so a flow says
  `sh ". lib/remote.sh"` instead of repeating twenty lines.

Recipes to write, all run inside the repository copy:

- `remote.sh`: a bare origin next to the copy, `origin` pointed at it, a second clone
  `../other` to push "someone else's" commits.
- `conflict.sh`: two branches that change the same line.
- `dirty.sh`: a known mix of modified, deleted, renamed, untracked and partly staged
  files.
- `hook.sh`: a `pre-commit` hook that fails.

## Which flows to run when

| A feature in | Run |
|---|---|
| layout, focus, panels (PLAN_1) | A1, A2, then the domain of the panel touched |
| diff, scroll (PLAN_3, PLAN_4) | A3, A4, B4, B5, B12 |
| staging (PLAN_6) | B1 to B12 |
| commit (PLAN_7) | C1 to C10, B1 |
| branches (PLAN_8) | D1 to D11, then E4 if push is affected |
| remote (PLAN_9) | E1 to E7 |
| stash (PLAN_10) | F1 to F5 |
| rebase, conflicts (PLAN_11) | G1 to G10, H1 to H4 |
| menus, config, mouse (PLAN_5, PLAN_12) | I1 to I4, L1 to L11 |
| scroll or selection behaviour (PLAN_4) | L2 to L4, L8, A3 |
