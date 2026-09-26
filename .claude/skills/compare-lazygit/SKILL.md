---
name: compare-lazygit
description: Compare a ferrit user flow with lazygit, behaviour and looks. Runs a flow (test/flows/*.flow) in both programs on a throwaway copy of the real repo, then writes the visual differences per step into the report. Use when a feature touches what the user sees or does and it must match lazygit, or when asked to compare with lazygit, check parity, or verify a flow visually.
---

# Compare a flow with lazygit

Mechanics live in `__SOP/visual-verify.md` (section "Comparing with lazygit"); do
not repeat them here. This skill is the procedure, and the judgement the
scripts cannot make.

## Procedure

1. Pick or write the flow in `test/flows/`. A real round trip (edit, stage,
   commit, branch...), not one screen. Reuse `feature-workflow.flow` when the
   change is not tied to a new action.
2. Run it on the real repo, never on the original:
   `.dev-tools/flow-compare.sh --repo . test/flows/<name>.flow`
   (needs Terminal.app, tmux, lazygit, Screen Recording permission; it opens
   windows and takes about a minute).
3. Read the git-state verdict of every step. A step marked "git state differs"
   is a behaviour difference: find the first one, everything after it follows.
   Check the flow itself is not at fault before blaming ferrit (a key that
   landed on a different row, a program that had not refreshed yet).
4. Look at every pair of screenshots (`.verify-shots/flows/<name>/lazygit/<step>.png`
   and `ferrit/<step>.png`), one step at a time.
5. Write `.verify-shots/flows/<name>/analysis/<step>.txt`, one `- ` bullet per
   difference, for every step (say "identical" when it is).
6. Write `.verify-shots/flows/<name>/implementation.txt`: the report the user
   acts on, rendered as tables at the end of the page (see "Implementation
   report" below for the format).
7. `.dev-tools/flow-report.sh <name>` rebuilds the report and opens it. This is the
   only step that opens it: `flow-compare.sh` builds it without opening, so the
   user first sees it with the analyses and the audit in place. Give the
   user the P1 items first, the path of the report, then stop.

## Fixing what the report found (the loop)

Only when the user asks to act on the audit. The tooling (flows, scripts, this
skill, the docs) is committed straight to `main`; a correction of ferrit's
behaviour goes through a branch, because it may not pan out.

1. One branch per fix, off an up-to-date `main`: `fix/<short-name>` (one row of the
   audit, or a few that share a cause). Uncommitted tooling work is committed to
   `main` first, so the branch starts clean.
2. Change ferrit on the branch, with tests, the CHANGELOG line and the plan file
   (AGENTS.md), and atomic commits. Never push the branch.
3. Rerun the same flow (`flow-compare.sh` builds the current branch's binary and
   keeps the run it replaces as `before/`), read the pairs again and rewrite
   `analysis/` and `implementation.txt`. Mark each row the fix touched FIXED with
   the step numbers where the screens now match, or STILL DIFFERS with what remains.
   Say when a row was not re-checked.
   Give every FIXED or PARTLY row a sixth cell, the step numbers to show before and
   after (`| ... | 8, 12 | 8, 12`): the report draws ferrit's old and new screen side
   by side there, so the fix can be seen without reading a counter. Pick the steps
   where the change is on screen, and name in the "ferrit" cell what to look at
   ("1 of 8" instead of "7 of 7"). Only ferrit's screen is shown: lazygit does not
   change between the two runs.
4. Judge the result with the user. Fixed and green (`cargo test`, clippy): merge
   into `main` locally (`git merge --ff-only` when possible), delete the branch.
   Not fixed or worse: leave the branch, say so, do not merge.
5. Start the next branch from the merged `main`. P3 rows are decisions: ask before
   changing them, and record the answer in the row.

## Implementation report

Built from the analyses, not from memory. Keep only what is inherent to the
lazygit experience: what the user does, sees, is told and can rely on. Sections,
in this order:

- `## P1 - breaks or misleads the workflow`: the selection or focus goes
  somewhere wrong, something the user needs is hidden, a key bar or pane does
  not match the view, an action does less than lazygit's.
- `## P2 - information the user relies on`: messages and git output, decorations,
  tags, colours that carry a state, counters and history length.
- `## P3 - parity to decide`: keys, startup focus, ordering, layout policy;
  ferrit may differ on purpose, so present them as decisions.
- `## P4 - look only`: sizes, popups, spacing.
- `## Left out on purpose`: what you dropped and why, so the user can contest it.

Drop, and list under "Left out": branding and support lines (Donate, Ask
Question, version, tips), ferrit's own identity (logo, its Infos box), pure
cosmetics with no effect on the workflow (title punctuation, glyphs, hash
length, age format, where a marker sits), and anything where ferrit adds to
lazygit rather than lacks something.

### Format

One `## P1 - title` heading per section, then one row per gap, five cells:

```
| What | lazygit | ferrit | To do | Seen in
```

Rows start with `| ` and separate cells with ` | `; no closing pipe, and no `|`
character inside a cell (write key bars and git stat lines with commas: a `|` in a
cell shifts every column after it). An optional sixth cell, `Before / after`, holds
step numbers (see the loop, step 3). "Seen in" is step numbers
separated by commas (`6, 8, 11`), rendered as links to those steps; anything
else (a flow name) stays text. "Left out on purpose" rows have two cells:
`| What | Why`.

Keep cells short: name the thing in "What" (a noun phrase, not a sentence), give
the visible fact on each side with its number or label in quotes, and make "To do"
an action ("Show git's output line"), or "Decide" for P3 and "Look only" for P4.
One row per gap: split a row if two fixes would be separate commits.

Say when a cause or a source flow was not verified in "ferrit" or "To do". A
difference seen in another flow is labelled as such in "Seen in".

## Stay inside the flow's scope

Each flow file has a `focus "..."` line (what it checks) and `not "..."` lines
(what it leaves to another flow); the report prints them at the top. Read them
before looking at any screenshot.

- A difference is analysed only if it bears on the focus. Panel sizes, popups,
  patch styling, launch screen and the like are noted only in the flow that has
  them in its focus (today `ui-mouse` for the interface itself).
- Something you notice outside the focus is not written in `analysis/` and does not
  go in the P1 to P4 tables. Add one row to "Left out on purpose", naming the flow
  that covers it ("Panel sizes | Covered by ui-mouse"), or nothing when no flow
  does. Never silently drop it and never re-analyse it.
- A step with nothing inside the focus gets `- nothing in this flow's scope`.
- A flow with no `focus` line gets one before it is run: ask what it is for.

## What to compare in each pair

- Behaviour visible on screen: what the selection does after an action, what
  stays visible (current branch, new commit), which pane shows what.
- Text: panel titles, counters ("1 of 224"), messages, popup titles, the key bar
  and the git command shown.
- Layout: panel sizes, popup size, what moves when the focus changes.
- Colour: markers, hashes, highlights, borders, dimming.

## Rules

- Write only what is on screen. If a difference might be a decision, say what
  differs and let the user decide; do not call it a bug. If you did not look at
  something, say so instead of implying it matches.
- One bullet, one difference, both sides named ("lazygit X; ferrit Y").
- lazygit is the reference, not the spec: ferrit may differ on purpose.
- The repo is a copy, its remotes point nowhere. Never point the flow at the
  original, never push, never commit the copy's changes.
- Do not commit `.dev-tools`, `test/flows` or analyses unless asked; do not add
  a CHANGELOG line for tooling.
- A rerun wipes the run and its analyses: write them after the last run.
