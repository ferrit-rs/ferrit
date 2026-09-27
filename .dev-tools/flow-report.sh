#!/usr/bin/env bash
# Dev-only helper: (re)build .verify-shots/flows/<name>/report.html from what
# flow-compare.sh left there. Three columns per step: lazygit, ferrit, and the
# visual differences someone (an agent, or you) wrote after looking at the two
# screenshots, in analysis/<step>.txt, one `- ` bullet per difference. At the end
# of the page, implementation.txt (`## P1 - title` headings, then `| a | b | c | d | e |`
# rows: What, lazygit, ferrit, To do, Seen in): what to
# build in ferrit, by priority, without what is not part of the lazygit experience.
#
# flow-compare.sh calls this at the end with --no-open (the analyses do not exist
# yet, so there is nothing worth showing); run it again once they are written, and
# it opens the report:
#   .dev-tools/flow-report.sh <name>
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
show=1
if [ "$1" = --no-open ]; then
  show=0
  shift
fi
name="$1"
out="$root/.verify-shots/flows/$name"

python3 - "$root/.dev-tools/report-template.html" "$out" "$name" "$root/test/flows/$name.flow" <<'PY'
import difflib, html, importlib.util, json, pathlib, re, shlex, sys

template, out, name, flow = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3], pathlib.Path(sys.argv[4])
out_dir = out
root_dir = pathlib.Path(template).parent.parent

_spec = importlib.util.spec_from_file_location("annotate", root_dir / ".dev-tools" / "annotate.py")
_annotate = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_annotate)


def marked_image(png, marks_path, color_override=None):
    """`png` unchanged if `marks_path` has no marks file; otherwise a `.annotated*.png`
    next to it, redrawn on every call (cheap, keeps it in sync with the marks file).
    `color_override` forces every mark's colour (used for a before/after pair, where
    the same marks.json is red on the old screen and green on the new one)."""
    if not marks_path.exists() or not png.exists():
        return png, None
    marks = json.loads(marks_path.read_text())
    if color_override:
        marks = [dict(m, color=color_override) for m in marks]
    suffix = f"-{color_override}" if color_override else ""
    annotated = png.with_name(f"{png.stem}.annotated{suffix}.png")
    _annotate.draw_marks(png, marks, annotated)
    numbers = {m["number"]: m.get("color", "red") for m in marks}
    return annotated, numbers


MARK_RE = re.compile(r"^\[(\d+)\]\s*")


def bullets(text, mark_colors):
    out = []
    for line in text.splitlines():
        if not line.startswith("- "):
            continue
        item = line[2:].strip()
        m = MARK_RE.match(item)
        badge = ""
        if m:
            number = m.group(1)
            color = mark_colors.get(int(number), "red") if mark_colors else "red"
            badge = f'<span class="mark {color}">{number}</span> '
            item = item[m.end():]
        out.append(f"<li>{badge}{html.escape(item)}</li>")
    return "<ul>%s</ul>" % "".join(out)


steps = sorted(p.name[: -len(".git.txt")] for p in (out / "lazygit").glob("*.git.txt"))

focus, not_here = [], []
if flow.exists():
    for line in flow.read_text().splitlines():
        words = shlex.split(line, comments=True) if line.strip() else []
        if len(words) >= 2 and words[0] == "focus":
            focus.append(words[1])
        elif len(words) >= 2 and words[0] == "not":
            not_here.append(words[1])
scope = ""
if focus or not_here:
    scope = '<div class="step scope"><h2>scope of this flow</h2>'
    scope += "".join(f"<p><b>Looks at:</b> {html.escape(f)}</p>" for f in focus)
    scope += "".join(f"<p><b>Does not look at:</b> {html.escape(n)}</p>" for n in not_here)
    scope += "</div>"

def stamp(path):
    """The screenshot's mtime: a browser keeps an image by its address, so a rerun
    that overwrites 08_x.png would keep showing the old one without this."""
    return int(path.stat().st_mtime) if path.exists() else 0


rows, differing, analysed = [scope] if scope else [], 0, 0
for step in steps:
    ref = (out / "lazygit" / f"{step}.git.txt").read_text().splitlines()
    got = (out / "ferrit" / f"{step}.git.txt").read_text().splitlines()
    diff = list(difflib.unified_diff(ref, got, "lazygit", "ferrit", lineterm="", n=1))
    if diff:
        differing += 1
    verdict = (
        '<span class="differs">git state differs</span><pre>%s</pre>' % html.escape("\n".join(diff))
        if diff
        else '<span class="same">same git state</span>'
    )
    note_file = out / "lazygit" / f"{step}.note.txt"
    note = f"<p>{html.escape(note_file.read_text().strip())}</p>" if note_file.exists() else ""

    lazygit_png, _ = marked_image(out / "lazygit" / f"{step}.png", out / "lazygit" / f"{step}.marks.json")
    ferrit_png, mark_colors = marked_image(out / "ferrit" / f"{step}.png", out / "ferrit" / f"{step}.marks.json")

    analysis_file = out / "analysis" / f"{step}.txt"
    if analysis_file.exists():
        analysed += 1
        analysis = bullets(analysis_file.read_text(), mark_colors)
    else:
        analysis = '<p class="muted">no analysis written for this step</p>'
    lazygit_rel = lazygit_png.relative_to(out)
    ferrit_rel = ferrit_png.relative_to(out)
    rows.append(
        f'<div class="step" id="{step}"><h2>{step}</h2>{note}{verdict}<div class="trio">'
        f'<figure><figcaption>lazygit</figcaption><img src="{lazygit_rel}?v={stamp(lazygit_png)}"></figure>'
        f'<figure><figcaption>ferrit</figcaption><img src="{ferrit_rel}?v={stamp(ferrit_png)}"></figure>'
        f'<div class="diffs"><div class="caption">visual differences</div>{analysis}</div>'
        f"</div></div>"
    )

BADGE = {"P1": "p1", "P2": "p2", "P3": "p3", "P4": "p4"}
HEADS = ["What", "lazygit", "ferrit", "Done when", "Seen in"]


def step_links(cell):
    """`6, 8, 11` becomes links to those steps; anything else stays text."""
    out = []
    for part in [c.strip() for c in cell.split(",")]:
        target = next((s for s in steps if part.isdigit() and s.startswith(f"{int(part):02d}_")), None)
        out.append(f'<a href="#{target}">{part}</a>' if target else html.escape(part))
    return ", ".join(out)


def proof_cell(cell):
    """`8, 12` becomes, per step, ferrit's screen before the fix next to the one after.
    The earlier run is kept by flow-compare.sh under before/."""
    out = []
    for part in [c.strip() for c in cell.split(",") if c.strip()]:
        step = next((s for s in steps if part.isdigit() and s.startswith(f"{int(part):02d}_")), None)
        old = out_dir / "before" / "ferrit" / f"{step}.png" if step else None
        new = out_dir / "ferrit" / f"{step}.png" if step else None
        if not (step and old.exists() and new.exists()):
            out.append(f"<div>step {html.escape(part)}: no earlier run kept</div>")
            continue
        marks_path = out_dir / "ferrit" / f"{step}.marks.json"
        old_shown, _ = marked_image(old, marks_path, color_override="red")
        new_shown, _ = marked_image(new, marks_path, color_override="green")
        out.append(
            f'<div class="ba"><b>step {part}</b><div class="pair2">'
            f'<figure><figcaption>before</figcaption><a href="{old_shown.relative_to(out_dir)}?v={stamp(old_shown)}"><img src="{old_shown.relative_to(out_dir)}?v={stamp(old_shown)}"></a></figure>'
            f'<figure><figcaption>after</figcaption><a href="{new_shown.relative_to(out_dir)}?v={stamp(new_shown)}"><img src="{new_shown.relative_to(out_dir)}?v={stamp(new_shown)}"></a></figure>'
            "</div></div>"
        )
    return "".join(out)


def implementation(text):
    sections, current = [], None
    for line in text.splitlines():
        if line.startswith("## "):
            current = (line[3:].strip(), [])
            sections.append(current)
        elif line.startswith("| ") and current:
            current[1].append([c.strip() for c in line.strip().strip("|").split(" | ")])
    counts = " &middot; ".join(
        f'<span class="badge {BADGE[t[:2]]}">{t[:2]}</span> {len(rows)}'
        for t, rows in sections
        if t[:2] in BADGE
    )
    out = [f'<p class="counts">{counts}</p>']
    for title, table in sections:
        code = title[:2]
        cls = BADGE.get(code, "left")
        out.append(f'<h3><span class="badge {cls}">{html.escape(code if code in BADGE else "out")}</span> '
                   f"{html.escape(title[5:] if code in BADGE else title)}</h3>")
        wide = table and len(table[0]) >= 5
        proof = wide and any(len(r) >= 6 for r in table)
        heads = (HEADS + ["Before / after"] if proof else HEADS) if wide else ["What", "Why"]
        out.append(f'<table class="audit {cls}{" proof" if proof else ""}"><thead><tr>'
                   + "".join(f"<th>{h}</th>" for h in heads) + "</tr></thead><tbody>")
        for row in table:
            cells = row + [""] * (len(heads) - len(row))
            tds = []
            for i, cell in enumerate(cells[: len(heads)]):
                if wide and i == 4:
                    tds.append(f'<td class="seen">{step_links(cell)}</td>')
                elif wide and i == 5:
                    tds.append(f'<td class="proof">{proof_cell(cell)}</td>')
                else:
                    tds.append(f'<td class="{"what" if i == 0 else ""}">{html.escape(cell)}</td>')
            out.append("<tr>" + "".join(tds) + "</tr>")
        out.append("</tbody></table>")
    return "".join(out)


impl_file = out / "implementation.txt"
impl = (
    implementation(impl_file.read_text())
    if impl_file.exists()
    else '<p class="muted">no implementation report written</p>'
)
rows.append(f'<div class="step impl"><h2>implementation report</h2>{impl}</div>')

title = (
    f"{name}: lazygit vs ferrit ({differing} of {len(steps)} steps differ in git state, "
    f"{analysed} analysed)"
)
page = open(template, encoding="utf-8").read()
page = page.replace("{{TITLE}}", html.escape(title)).replace("{{STEPS}}", "".join(rows))
(out / "report.html").write_text(page, encoding="utf-8")
print(title)
PY

echo "saved $out/report.html"
# A page already open in a tab is not reloaded by `open`, so each build that opens
# gets a copy under a new name: a new name is a new tab, showing this run.
if [ "$show" = 1 ]; then
  copy="$out/report-$(date +%H%M%S).html"
  cp "$out/report.html" "$copy"
  echo "REPORT: $copy"
  open "$copy"
fi
