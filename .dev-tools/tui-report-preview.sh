#!/usr/bin/env bash
# Dev-only helper: render report-template.html with neutral placeholder
# boxes and lorem ipsum instead of real screenshots, so the report's look
# (sizing, theme toggle, spacing) can be tuned by editing the template alone,
# with no tmux/ferrit/tui-shot run needed. Tracked in git.
#
# Usage: .dev-tools/tui-report-preview.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
template="$root/.dev-tools/report-template.html"
preview="$root/.dev-tools/report-preview.html"

lorem="Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua, exactly the length a real step note tends to run."

steps=""
for step in 01_start 02_focus 03_drill; do
  steps="$steps<div class=\"step\"><h2>$step</h2><p>$lorem</p><div class=\"placeholder\">screenshot placeholder</div></div>"
done

python3 - "$template" "$preview" "$steps" <<'PY'
import sys
template_path, preview_path, steps = sys.argv[1:4]
html = open(template_path, encoding="utf-8").read()
html = html.replace("{{TITLE}}", "report template preview").replace("{{STEPS}}", steps)
open(preview_path, "w", encoding="utf-8").write(html)
PY

echo "saved $preview"
open "$preview"
