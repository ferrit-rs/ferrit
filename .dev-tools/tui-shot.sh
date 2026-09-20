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
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
shots="$root/.verify-shots"

name="$1"
shift
mkdir -p "$shots/$(dirname "$name")"

if ! tmux has-session -t ferrit 2>/dev/null; then
  # FERRIT_NO_GRAPHICS: skips the terminal graphics-capability query, which
  # blocks on stdio waiting for an answer no one sends under headless tmux
  # and leaves raw mode broken for the rest of the run once it gives up.
  tmux new-session -d -s ferrit -x 200 -y 50 "cd '$root' && FERRIT_NO_GRAPHICS=1 ./target/debug/ferrit"
  # Cold start (cargo-built binary, first paint): give it real time, then
  # poll until the alt-screen has actually drawn something instead of
  # trusting a fixed sleep (a blank first frame was the earlier bug here).
  for _ in $(seq 1 20); do
    sleep 0.2
    if tmux capture-pane -t ferrit -p | grep -q '[^[:space:]]'; then
      break
    fi
  done
fi

for key in "$@"; do
  case "$key" in
    Enter | Escape | Up | Down | Left | Right | Tab | BTab | PageUp | PageDown | Space)
      tmux send-keys -t ferrit "$key"
      ;;
    *)
      tmux send-keys -t ferrit -l "$key"
      ;;
  esac
  sleep 0.4
done

tmux capture-pane -t ferrit -p -e > "$shots/$name.txt"

python3 - "$shots/$name.txt" "$shots/$name.html" <<'PY'
import re
import sys

src, dst = sys.argv[1], sys.argv[2]
text = open(src, encoding="utf-8", errors="replace").read()

# 256-color xterm palette -> hex, just enough (0-15 ANSI + 16-231 cube + gray
# ramp) to render ratatui's `Color::Indexed` output faithfully.
def color256(n):
    if n < 16:
        base = [
            "#000000", "#cd0000", "#00cd00", "#cdcd00", "#0000ee", "#cd00cd",
            "#00cdcd", "#e5e5e5", "#7f7f7f", "#ff0000", "#00ff00", "#ffff00",
            "#5c5cff", "#ff00ff", "#00ffff", "#ffffff",
        ]
        return base[n]
    if n < 232:
        n -= 16
        r, g, b = n // 36, (n // 6) % 6, n % 6
        scale = lambda v: 0 if v == 0 else 55 + v * 40
        return f"#{scale(r):02x}{scale(g):02x}{scale(b):02x}"
    gray = 8 + (n - 232) * 10
    return f"#{gray:02x}{gray:02x}{gray:02x}"

ansi_re = re.compile(r"\x1b\[([0-9;]*)m")

out = []
pos = 0
bold = False
fg = None
for m in ansi_re.finditer(text):
    out.append((text[pos:m.start()], bold, fg))
    pos = m.end()
    codes = [c for c in m.group(1).split(";") if c != ""] or ["0"]
    i = 0
    while i < len(codes):
        c = codes[i]
        if c == "0":
            bold, fg = False, None
        elif c == "1":
            bold = True
        elif c == "39":
            fg = None
        elif c == "38" and i + 2 < len(codes) and codes[i + 1] == "5":
            fg = color256(int(codes[i + 2]))
            i += 2
        elif c.isdigit() and 30 <= int(c) <= 37:
            fg = color256(int(c) - 30)
        i += 1
out.append((text[pos:], bold, fg))

def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

spans = []
for chunk, b, color in out:
    if not chunk:
        continue
    style = []
    if color:
        style.append(f"color:{color}")
    if b:
        style.append("font-weight:bold")
    if style:
        spans.append(f'<span style="{";".join(style)}">{esc(chunk)}</span>')
    else:
        spans.append(esc(chunk))

page = f"""<!doctype html><html><head><meta charset="utf-8">
<style>body{{background:#111;margin:0;padding:12px}}
pre{{font-family:'Menlo','Courier New',monospace;font-size:13px;line-height:1.15;white-space:pre;color:#ddd}}
</style></head><body><pre>{''.join(spans)}</pre></body></html>"""
open(dst, "w").write(page)
PY

"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --disable-gpu --no-sandbox \
  --screenshot="$shots/$name.png" --window-size=1600,900 \
  "file://$shots/$name.html" >/dev/null 2>&1

echo "saved $shots/$name.png"
