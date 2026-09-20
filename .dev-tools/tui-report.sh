#!/usr/bin/env bash
# Dev-only helper: bundle every screenshot a tui-shot.sh run saved under a
# feature's .verify-shots/<feature>/ folder into one report.html (built from
# report-template.html), then open it. Tracked in git like tui-shot.sh;
# .verify-shots/ itself stays ignored.
#
# Usage: .dev-tools/tui-report.sh <feature>
#   feature  the subfolder under .verify-shots/ that tui-shot.sh wrote into,
#            e.g.: .dev-tools/tui-shot.sh commit-drill/01_start
#                  .dev-tools/tui-shot.sh commit-drill/02_focus 4
#                  .dev-tools/tui-shot.sh commit-drill/03_drill Enter
#                  .dev-tools/tui-report.sh commit-drill
#
# A step gets an explanation paragraph in the report by dropping a
# <step>.note.txt file next to its .png (plain text, one or more sentences on
# what the step does and why); a step without one just shows its screenshot.
#
# Edit report-template.html to change how every future report looks (image
# size, colors, dark/light theme, layout) — this script only fills in
# {{TITLE}} and {{STEPS}}.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
feature="$1"
dir="$root/.verify-shots/$feature"
report="$dir/report.html"
template="$root/.dev-tools/report-template.html"

shopt -s nullglob
pngs=("$dir"/*.png)
shopt -u nullglob
if [ ${#pngs[@]} -eq 0 ]; then
  echo "no .png files under $dir (run tui-shot.sh $feature/<step> first)" >&2
  exit 1
fi

steps=""
for png in "${pngs[@]}"; do
  step="$(basename "$png" .png)"
  note="$dir/$step.note.txt"
  note_html=""
  if [ -f "$note" ]; then
    note_html="<p>$(sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g' "$note")</p>"
  fi
  steps="$steps<div class=\"step\"><h2>$step</h2>$note_html<img src=\"$(basename "$png")\"></div>"
done

python3 - "$template" "$report" "$feature" "$steps" <<'PY'
import sys
template_path, report_path, feature, steps = sys.argv[1:5]
html = open(template_path, encoding="utf-8").read()
html = html.replace("{{TITLE}}", feature).replace("{{STEPS}}", steps)
open(report_path, "w", encoding="utf-8").write(html)
PY

echo "saved $report"
open "$report"
