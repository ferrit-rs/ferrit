# SOP: improve a `PLAN_N` before it is implemented

Plan exists, no code written yet. Cost of change is near zero now and high
later, so scrutinise hard. But do not grow the phase: improvements that add
scope go to "Out of scope" as named follow-ups.

## Do

1. Re-read the whole plan.
2. Read the code it targets, and `Cargo.lock` for the exact version of every
   API it leans on (`ratatui`, ...). New minor versions add helpers the plan
   may be hand-rolling.
3. Sweep `../ferrit-references/` for the same pattern in the **same API**
   (ratatui refs beat Go refs). Get `path:line`.
4. Walk the plan's approach against those refs, one step at a time. Flag:
   - hand-rolled code where the stdlib / crate already has it
     (`Rect::contains` vs a manual bounds check).
   - fragile invariants ("valid only after render", parallel state that can
     drift).
   - return-type / helper ergonomics (`-> bool` vs `Option` + caller
     assignment; one method vs spread across the handler).
   - missing explicit arms (a `_ => return` swallowing a case that deserves
     its own `// phase N` line).
   - borrow-order / ownership gotchas in the render sketch.
5. Fold in the cheap, no-behaviour-change fixes. Anything that changes
   behaviour or touches other phases: move to "Out of scope" with the reason
   and where it lands.
6. Propagate. A changed helper name or signature must update every dependent
   section: the ASCII schema, impl sketch, self-testing bullets, milestones,
   definition of done, "After phase N".

## Keep

- The plan's structure, voice, and ASCII schemas. Edit in place, do not
  rewrite.
- No em dashes, no `---` rules.
- Every new claim about a reference carries `path:line`.

## Ship

- Docs only, no `CHANGELOG` line.
- Commit straight to `main`: `docs(plan): PLAN_<N> <what changed>`.
