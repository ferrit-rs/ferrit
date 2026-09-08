# SOP: create the next `docs/PLAN_N_*.md`

Run this every time a new phase plan is needed. One phase = one
`PLAN_N_<TOPIC>.md` = one mergeable slice. Keep it small, referenced, and
testable. Copy the shape of `PLAN_4` / `PLAN_5`, they are the template.

`N` = next free number. Filename `PLAN_<N>_<SHORT_TOPIC>.md`, all caps topic
(`PLAN_8_BRANCH_ACTIONS.md`). If it slots between existing phases, renumber
the later ones in a separate commit (`docs(plan): renumber ...`).

## Before writing

1. Read the 2 or 3 nearest existing plans and the code they touch.
2. Find the gesture / feature in `../ferrit-references/` (lazygit, gitu,
   gitpane, drydock, gitui, ...). Prefer the ratatui refs, they hit ferrit's
   real API. Note exact `path:line`.
3. Decide the single smallest visible change. Push everything else to
   "Out of scope".

## Required sections, in order

- **Goal**: one paragraph, the obvious user-facing behaviour. Name the
  reference apps that do it the same way.
- **The gap this fixes**: the current code, quoted, with the line where it
  stops short.
- **Approach**: the reference implementation(s), quoted or paraphrased tight,
  with `path:line`. State which steps ferrit keeps and which it drops, each
  with a reason.
- **What it has to resolve**: the non-obvious inputs (rects, offsets, borders,
  non-model rows). One ASCII screen schema.
- **Coordinate / decision schema**: ASCII data-flow or decision tree for the
  core mapping. This is the spec, code sketches follow it.
- **State on `App`**: new fields (type + one-line doc each), test/render seams
  named like the existing ones (`set_*`), any `&App -> &mut App` change and
  the borrow-order note.
- **Impl sketch**: `on_*` handler as Rust, real function names, `// step N`
  comments tying back to the reference order. Helpers as small `fn`s.
- **Out of scope**: bulleted, bold lead, each with where it lands later
  (phase number) and why it is safe to defer.
- **Self-testing** (see `PLAN_SELF_TESTING.md`): `tests/*.rs` file, the seams
  it drives, one bullet per case. Cover: happy path, border/title, past-tail,
  gap/no-hit, overlay up, wrong input kind, prior-phase cases still green.
- **Milestones**: `C0..Cn`. C0 = state + seams, no behaviour change, existing
  tests green. Middle = behaviour + its tests. Last = `cargo clippy
  --all-targets` clean, no-ops verified, layering rule held
  (`src/git/` has no `ratatui`), all prior C green.
- **Definition of done**: flat checklist a reviewer runs. Every "does
  nothing" case listed explicitly. clippy + named test files pass.
- **After phase N**: the next plan, and the hook this one leaves for it.

## Rules

- ASCII schemas for anything spatial or branching. UI changes get a UI
  schema. Cheaper to read than prose.
- Every claim about a reference carries `path:line`.
- Quote real ferrit identifiers (`App::update_right_pane`, `Pane`, field
  names), not invented ones. Grep to confirm they exist.
- No em dashes, no `---` rules (see `CLAUDE.md`). Commas, colons, parens.
- Read-only stays read-only until the plan that changes that says so.
- A visible change adds a `## [Unreleased]` line in `CHANGELOG.md`; a plan
  doc alone does not.

## Ship

- Commit straight to `main`: `docs(plan): add PLAN_<N> <topic>`. No branch,
  no PR.
