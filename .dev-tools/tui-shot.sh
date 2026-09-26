#!/usr/bin/env bash
# Dev-only helper, not part of ferrit itself: send keys to the tmux-hosted
# ferrit TUI and save a real PNG screenshot of the result under .verify-shots/.
# Tracked in git (unlike .verify-shots/, which stays ignored); exists to give
# visual feedback without needing macOS Screen Recording permission.
#
# Usage: .dev-tools/tui-shot.sh <name> [key...]
#   name  base filename for the .txt/.html/.png outputs; may include a
#         subfolder (e.g. commit-drill/01_start) to group a feature's steps
#         for tui-report.sh
#   key   zero or more tmux send-keys arguments, sent literally (-l) one at a
#         time, e.g.: .dev-tools/tui-shot.sh pane4 4 Enter
#
# Special key names ("Enter", "Escape", "Up", ...) are sent WITHOUT -l so
# tmux resolves them as keysyms; anything else is sent literally.
#
# The screenshot itself comes from macOS `screencapture` on a real Terminal.app
# window attached to the tmux session (System Settings -> Privacy & Security ->
# Screen Recording must be granted to the app hosting this shell). This gives
# a pixel-perfect native render instead of the old ANSI-to-HTML-to-headless-
# Chrome reconstruction. The window is opened once and reused across calls; its
# id is cached in .verify-shots/.terminal_window_id.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
shots="$root/.verify-shots"
# FERRIT_SHOT_SESSION / FERRIT_SHOT_CMD let flow-compare.sh drive lazygit next
# to ferrit: one tmux session and one Terminal.app window per program.
session="${FERRIT_SHOT_SESSION:-ferrit}"
cmd="${FERRIT_SHOT_CMD:-cd '$root' && FERRIT_NO_GRAPHICS=1 ./target/debug/ferrit}"
winfile="$shots/.terminal_window_id"
[ "$session" = ferrit ] || winfile="$winfile.$session"

name="$1"
shift
mkdir -p "$shots/$(dirname "$name")"

if ! tmux has-session -t "$session" 2>/dev/null; then
  # A new session needs a new window: the cached one is attached to a dead one.
  if [ -f "$winfile" ]; then
    osascript -e "tell application \"Terminal\" to close window id $(cat "$winfile")" >/dev/null 2>&1 || true
    rm -f "$winfile"
  fi
  # FERRIT_NO_GRAPHICS: skips the terminal graphics-capability query, which
  # blocks on stdio waiting for an answer no one sends under headless tmux
  # and leaves raw mode broken for the rest of the run once it gives up.
  tmux new-session -d -s "$session" -x 200 -y 50 "$cmd"
  # Cold start (cargo-built binary, first paint): give it real time, then
  # poll until the alt-screen has actually drawn something instead of
  # trusting a fixed sleep (a blank first frame was the earlier bug here).
  for _ in $(seq 1 20); do
    sleep 0.2
    if tmux capture-pane -t "$session" -p | grep -q '[^[:space:]]'; then
      break
    fi
  done
fi

winid=""
[ -f "$winfile" ] && winid="$(cat "$winfile")"
if [ -z "$winid" ] || ! osascript -e "tell application \"Terminal\" to exists window id $winid" 2>/dev/null | grep -q true; then
  winid="$(osascript -e "tell application \"Terminal\" to do script \"tmux attach -t $session\"" \
                      -e 'delay 0.5' \
                      -e 'tell application "Terminal" to id of front window')"
  osascript -e "tell application \"Terminal\"
    set number of columns of window id $winid to 200
    set number of rows of window id $winid to 50
  end tell" >/dev/null
  echo "$winid" > "$winfile"
  sleep 0.5
fi

for key in "$@"; do
  case "$key" in
    Enter | Escape | Up | Down | Left | Right | Tab | BTab | PageUp | PageDown | Space | BSpace | C-?)
      tmux send-keys -t "$session" "$key"
      ;;
    click:* | wheel:* | scrollbar:* | dragbar:*)
      # A mouse gesture, by panel name: see tui-mouse.py.
      "$root/.dev-tools/tui-mouse.py" "$session" "$key"
      ;;
    text:*)
      # text:<string> types the string as is, spaces included.
      tmux send-keys -t "$session" -l "${key#text:}"
      ;;
    *)
      tmux send-keys -t "$session" -l "$key"
      ;;
  esac
  sleep 0.4
done

# Wait for the screen to stop changing (two identical captures in a row), so a
# program that redraws asynchronously, lazygit, is shot once it has settled.
prev=""
for _ in $(seq 1 30); do
  cur="$(tmux capture-pane -t "$session" -p -e)"
  [ "$cur" = "$prev" ] && break
  prev="$cur"
  sleep 0.15
done

screencapture -x -l "$winid" "$shots/$name.png"

echo "saved $shots/$name.png"
