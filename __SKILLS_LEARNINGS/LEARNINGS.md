# Learnings

One line per learning. Format: `YYYY-MM-DD [domain] avoid X, do Y, because Z`.

New captures land in **Inbox** (not yet reviewed). Move a line up to
**Confirmed** once validated as worth keeping. Delete from Inbox what you do
not want.

## Confirmed



## Inbox

- 2026-09-21 [ferrit/communication] avoid explaining visual UI behavior only in prose, show an ASCII diagram because it makes the intended layout or interaction clear.
- 2026-09-20 [ferrit/changes] avoid removing existing features during improvements; preserve behavior and add or improve capabilities, because refactors must not silently reduce Ferrit's feature set.
- 2026-09-20 [ferrit/ui-architecture] prefer `src/components/ui/` for reusable UI primitives, because it matches shadcn's discoverable component registry layout.
- 2026-09-20 [ferrit/tui-verify] avoid claiming a TUI keypress/feature works from ASCII text alone, do drive it headless via `.dev-tools/tui-shot.sh` + `.dev-tools/tui-report.sh` (see `__SOP/visual-verify.md`) and actually look at the rendered screenshot before reporting success, because plain-text tmux captures strip color/focus cues and a real bug (`FERRIT_NO_GRAPHICS` graphics-query hang breaking raw mode) was invisible without the real screenshot.
