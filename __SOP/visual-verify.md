# Visual verification of a TUI feature

Drive ferrit headless under tmux, capture a real screenshot of it with
macOS `screencapture` on an attached Terminal.app window, bundle the steps
into one HTML report, open it. Needs Screen Recording permission (System
Settings -> Privacy & Security -> Screen Recording) granted to whatever app
hosts this shell (fully quit and restart it after granting).

## Steps

1. Build: `cargo build --quiet`
2. Kill any stale session: `tmux kill-session -t ferrit 2>/dev/null; true`
3. Shoot each step of the flow, one call per step, name them
   `<feature>/<NN_step>`:
   ```
   .dev-tools/tui-shot.sh <feature>/01_start
   .dev-tools/tui-shot.sh <feature>/02_focus 4
   .dev-tools/tui-shot.sh <feature>/03_drill Enter
   ```
   The first call starts the tmux session (`FERRIT_NO_GRAPHICS=1`, see
   Gotcha below) and opens/resizes the Terminal.app window used for
   capture (cached in `.verify-shots/.terminal_window_id`); later calls
   reuse both. Extra args are keys sent via `tmux send-keys`, one per key,
   in order.
4. For each step, write a one-paragraph `<feature>/<NN_step>.note.txt` next
   to its `.png`: what was pressed, what should change on screen, and why
   that proves the feature works. This is what turns the report into
   something a human can actually read instead of a pile of screenshots.
5. Build and open the report: `.dev-tools/tui-report.sh <feature>` — writes
   `.verify-shots/<feature>/report.html` and opens it.
6. Actually look at it. Check colors, borders, titles, focus indicators —
   don't just confirm a file exists.
7. `tmux kill-session -t ferrit 2>/dev/null; true` when done.

## Gotcha: keys silently do nothing

If every keypress appears to do nothing (focus never changes, screen never
updates), the terminal graphics-capability query
(`Picker::from_query_stdio`, called from `App::detect_graphics` before raw
mode is even on) is blocking on stdio waiting for an answer nobody sends
under headless tmux, and leaves raw mode broken once it gives up. Fix:
launch ferrit with `FERRIT_NO_GRAPHICS=1` (already baked into
`tui-shot.sh`). If you ever see this again with a different symptom, verify
with a raw ANSI diff (`tmux capture-pane -t ferrit -p -e`, diff two
captures around a keypress) before assuming the feature itself is broken.

## Files

- `.dev-tools/tui-shot.sh` — one step, one screenshot. Tracked in git.
- `.dev-tools/tui-report.sh` — bundles a feature folder's screenshots +
  notes into `report.html` and opens it. Tracked in git.
- `.dev-tools/report-template.html` — the report's HTML/CSS (image size,
  dark/light theme toggle, layout). Edit this to change how every future
  report looks; `tui-report.sh` only fills in `{{TITLE}}`/`{{STEPS}}`.
- `.dev-tools/tui-report-preview.sh` — renders the template with neutral
  placeholder boxes and lorem ipsum instead of real screenshots, so the
  template's look can be tuned with no tmux/ferrit run needed. Writes
  `.dev-tools/report-preview.html` (tracked in git) and opens it.
- `.verify-shots/` — all output (`.txt`/`.html`/`.png`/notes/reports).
  Gitignored, ephemeral, safe to delete anytime.

## Comparing with lazygit: `test/flows/*.flow`

`.dev-tools/flow-compare.sh test/flows/<name>.flow` runs one flow (a fixture,
then `step NAME KEYS...` lines) in lazygit and in ferrit, each in its own tmux
session and Terminal.app window with an empty `HOME`, shoots every step with
`tui-shot.sh` (`FERRIT_SHOT_SESSION` / `FERRIT_SHOT_CMD` pick the program) and
builds `.verify-shots/flows/<name>/report.html` (it does not open it: the analyses are not there yet): lazygit | ferrit per step, plus
a diff of the git state (status, log subjects, branches, stash, index) each one
left. lazygit is the reference: "git state differs" is a workflow that does not
behave the same, the screenshots are for judging the look. It fails nothing.

Against a real repository: `--repo .` (or a `repo .` line) works on a throwaway
copy (history, branches, stash, WIP; remotes pointed at nothing, your global git
config kept), never the original. `sh "cmd"` runs in the copy: before the first
step it is setup (`git reset --hard && git clean -fd` for a known start), after
it is an edit behind the programs' back, followed by a 2 s wait because lazygit
rereads the disk every `refreshInterval` (set to 1 s here). `note "text"` puts a
line in the report for the next step. `test/flows/feature-workflow.flow` is the
full round trip: edit, stage, commit, new branch, commit, review.

The report has three columns per step: lazygit, ferrit, visual differences. The
third is written after the run, by looking at each pair of screenshots: one
`- ` bullet per difference in `.verify-shots/flows/<name>/analysis/<step>.txt`
(layout, colours, wording, counters, key bars, what the selection does), then
`.dev-tools/flow-report.sh <name>` rebuilds and opens the report. At the end of the page, `implementation.txt` is the report to act on, drawn as
tables: differences sorted P1 (breaks the workflow) to P4 (look only), one row per
gap (What, lazygit, ferrit, To do, Seen in, the steps being links), without what
is not part of the lazygit experience, plus a "left out on purpose" table. A step without
a file shows "no analysis written". A rerun wipes the run, analyses included,
because they describe those screenshots.

Mouse gestures are steps too, named after panels, not coordinates (`tui-mouse.py`):
`click:commits:4`, `wheel:commits:down:3`, `scrollbar:commits:50`,
`dragbar:commits:0:100`; `main` is the right pane. Each waits 1 s so a background
load finishes before the shot. `test/flows/ui-mouse.flow` is the tour.

Scope: `focus "..."` says what the flow checks and `not "..."` what it leaves to
another flow. The report prints both first, and the analysis and the audit stay
inside the focus (see the skill), so the interface's own behaviour is judged in
`ui-mouse` once, not again in every git flow.

Step keys are `tui-shot.sh` keys (`Enter`, `Space`, `C-x`, `text:<string>`).
The file tree starts on the `/` root row, so the first `j` lands on the first
entry. When ferrit deliberately uses another key, that step needs its own
keys per program (not supported yet).
