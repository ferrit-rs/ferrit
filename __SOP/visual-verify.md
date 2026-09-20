# Visual verification of a TUI feature

macOS Screen Recording permission isn't available here, so `screencapture`
is blocked. This is the workaround: drive ferrit headless under tmux, render
a real screenshot with headless Chrome, bundle the steps into one HTML
report, open it.

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
   Gotcha below); later calls reuse it. Extra args are keys sent via
   `tmux send-keys`, one per key, in order.
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
