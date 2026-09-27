#!/usr/bin/env python3
"""Dev-only helper: draw numbered boxes on a tui-shot.sh screenshot, from a
JSON sidecar describing what to point at, in terminal cells (row/col), not
pixels. Red for an open gap, green for one confirmed fixed.

Calibration (`.dev-tools/tui-shot.sh`'s fixed 200x50 Terminal.app window,
screencapture on this machine): content starts at pixel (77, 114) and each
cell is 14.105 x 28.66px. Recalibrate CELL if the capture geometry changes
(a different screen, font size, or terminal size) — see the skill for how.

Usage:
  annotate.py SCREENSHOT.png ANNOTATIONS.json OUT.png

ANNOTATIONS.json: a list of
  {"row": 12, "col": 40, "rows": 1, "cols": 30, "number": 1, "color": "red"}
row/col are 0-based terminal cells (row 0 = top of the content area, inside
the window's own border); "rows"/"cols" default to 1. "color" is "red" or
"green".
"""
import json
import sys

from PIL import Image, ImageDraw, ImageFont

LEFT, TOP = 77.0, 114.0
CELL_W, CELL_H = 14.105, 28.66

COLORS = {
    "red": (229, 83, 75),
    "green": (63, 185, 80),
}


def cell_rect(row, col, rows=1, cols=1):
    x0 = LEFT + col * CELL_W
    y0 = TOP + row * CELL_H
    x1 = LEFT + (col + cols) * CELL_W
    y1 = TOP + (row + rows) * CELL_H
    return x0, y0, x1, y1


def badge_font(size):
    try:
        return ImageFont.truetype(
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf", size
        )
    except OSError:
        return ImageFont.load_default()


def draw_marks(screenshot_path, marks, out_path):
    """`marks`: a list of {row, col, rows, cols, number, color}, already loaded (see the
    module docstring for the shape). Used both by the CLI below and by flow-report.sh,
    which calls this directly (no subprocess, no round trip through a temp JSON file)."""
    im = Image.open(screenshot_path).convert("RGB")
    draw = ImageDraw.Draw(im)
    font = badge_font(30)
    for note in marks:
        color = COLORS[note.get("color", "red")]
        x0, y0, x1, y1 = cell_rect(
            note["row"], note["col"], note.get("rows", 1), note.get("cols", 1)
        )
        pad = 3
        box = (x0 - pad, y0 - pad, x1 + pad, y1 + pad)
        draw.rectangle(box, outline=color, width=4)
        number = str(note["number"])
        r = 18
        cx, cy = box[0], box[1]
        draw.ellipse((cx - r, cy - r, cx + r, cy + r), fill=color, outline=(0, 0, 0))
        bbox = draw.textbbox((0, 0), number, font=font)
        tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
        draw.text(
            (cx - tw / 2 - bbox[0], cy - th / 2 - bbox[1]),
            number,
            fill=(255, 255, 255),
            font=font,
        )
    im.save(out_path)


def draw(screenshot_path, annotations_path, out_path):
    """CLI entry point: `marks` read from a JSON file instead of passed in memory."""
    marks = json.loads(open(annotations_path, encoding="utf-8").read())
    draw_marks(screenshot_path, marks, out_path)


if __name__ == "__main__":
    draw(sys.argv[1], sys.argv[2], sys.argv[3])
