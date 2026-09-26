#!/usr/bin/env python3
"""Dev-only helper: send one mouse gesture to a tmux session, as the SGR mouse
escape sequences a terminal would send. Panels are found by their "[N]" titles
in the captured screen, so a gesture names a panel, not coordinates.

  tui-mouse.py SESSION click:PANEL[:ROW]            left click, ROW-th row (default 1)
  tui-mouse.py SESSION wheel:PANEL:up|down:N        N wheel ticks over the panel
  tui-mouse.py SESSION scrollbar:PANEL:PERCENT      click the scrollbar column at PERCENT of the height
  tui-mouse.py SESSION dragbar:PANEL:FROM:TO        press at FROM percent, drag to TO percent, release

PANEL is status, files, branches, commits, stash or main (the right pane).
"""
import re
import subprocess
import sys
import time

NUM = {"status": 1, "files": 2, "branches": 3, "commits": 4, "stash": 5}


def send(session, seq):
    subprocess.run(["tmux", "send-keys", "-t", session, "-l", seq], check=True)
    time.sleep(0.03)


def screen(session):
    out = subprocess.run(["tmux", "capture-pane", "-t", session, "-p"], capture_output=True, text=True, check=True)
    return out.stdout.split("\n")


def geometry(lines, panel):
    """(left column, top row, bottom row, right edge column), 1-based, inside the panel."""
    if panel == "main":
        return 100, 4, 20, 120
    titles = {}
    for row, line in enumerate(lines, 1):
        m = re.match(r"^.{0,3}\[(\d)\]", line[:100].replace("─", " ")) or re.search(r"[┌╭]\S*\[(\d)\]", line[:100])
        if m and int(m.group(1)) in NUM.values() and int(m.group(1)) not in titles:
            titles[int(m.group(1))] = row
    n = NUM[panel]
    top = titles[n] + 1
    later = [r for k, r in titles.items() if r > titles[n]]
    bottom = (min(later) - 2) if later else top + 8
    edge = max(i for i, ch in enumerate(lines[titles[n] - 1][:100], 1) if ch in "┐╮")
    return 10, top, max(top, bottom), edge


def main():
    session, gesture = sys.argv[1], sys.argv[2].split(":")
    kind, panel = gesture[0], gesture[1]
    col, top, bottom, edge = geometry(screen(session), panel)
    at = lambda pct: top + round((bottom - top) * int(pct) / 100)
    if kind == "click":
        row = top + int(gesture[2] if len(gesture) > 2 else 1) - 1
        send(session, f"\x1b[<0;{col};{row}M")
        send(session, f"\x1b[<0;{col};{row}m")
    elif kind == "wheel":
        code = 64 if gesture[2] == "up" else 65
        for _ in range(int(gesture[3])):
            send(session, f"\x1b[<{code};{col};{(top + bottom) // 2}M")
    elif kind == "scrollbar":
        row = at(gesture[2])
        send(session, f"\x1b[<0;{edge};{row}M")
        send(session, f"\x1b[<0;{edge};{row}m")
    elif kind == "dragbar":
        start, end = at(gesture[2]), at(gesture[3])
        send(session, f"\x1b[<0;{edge};{start}M")
        step = 1 if end >= start else -1
        for row in range(start + step, end + step, step):
            send(session, f"\x1b[<32;{edge};{row}M")
        send(session, f"\x1b[<0;{edge};{end}m")
    else:
        sys.exit(f"unknown gesture {sys.argv[2]}")
    # Both programs load diffs and refresh in the background: give them time before
    # the next gesture, or a fast one lands on a screen that is not ready.
    time.sleep(1.0)


main()
