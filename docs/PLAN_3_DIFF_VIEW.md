# Plan: phase 3, diff view

## Goal

Right pane must reproduce lazygit's configured delta pager: commit header,
commit subject, file stat bars, aggregate shortstat, `Δ` file marker, blue
separator, line-number gutter, word-level changed spans, and unified diff
body. Changed-line background and syntax colour remain mutually exclusive.

## Target rendering

Context lines keep per-token syntax colour on plain background. A `+`/`-` line
gets full-width background with flat/plain text, no per-token colour:

```
  444:450    self.left_areas[pane] = area;                 <- context: syntax colour, no bg
  445:451    }
 ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
 ▓   :453    pub fn right_focused(&self) -> bool {         <- changed: full bg, flat text
 ▓   :456        self.right_focused
 ▓   :457    }
 ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
  449:461    pub fn list_offset(&self, pane: Pane) -> usize <- back to syntax colour
```

`old:new│` gutter, shortstat summary line, and boxed hunk/file focus
highlight (from earlier work) are unaffected by this change.

## LazyGit visual contract

Lazygit does not show raw git show output directly. It presents structured
patch output:

```text
┌─ Patch ────────────────────────────────────────────────────────────────┐
│ commit db9cff48a827f579225f4bd134429d2d06046267                      │
│ Author: Max Wells <maxwells@proton.me>                                 │
│ Date:   Fri Sep 11 15:04:14 2026 +0200                                │
│                                                                        │
│ feat(diff): match pager rendering and cache styled output              │
│ ---                                                                    │
│ CHANGELOG.md             | 6 ++++                                      │
│ docs/PLAN_3_DIFF_VIEW.md | 25 ++++++++++++++++-----                    │
│ src/app.rs               | 50 +++++++++++++++++++++++++++++++++++++++  │
│ src/theme.rs             | 121 ++++++++++++++++++++++++++++++++++++++-- │
│ src/ui.rs                | 38 ++++++++++++++++++------                 │
│ 5 files changed, 155 insertions(+), 85 deletions(-)                  │
│                                                                        │
│ Δ CHANGELOG.md                                                         │
│ ────────────────────────────────────────────────────────────────────── │
│  9:9   ## Unreleased                                                    │
│ 10:10  ### Added                                                        │
│ 11:11  - Existing entry                                                 │
│       + New entry                                                       │
│ 12:12  - Context line                                                   │
│                                                                        │
│ Δ docs/PLAN_3_DIFF_VIEW.md                                              │
│ ────────────────────────────────────────────────────────────────────── │
│  ...                                                                   │
└────────────────────────────────────────────────────────────────────────┘
```

### Header order

```text
Patch
├── commit <full hash>
├── Author: <name and email>
├── Date:   <formatted date>
├── blank line
├── <commit subject, wrapped when needed>
├── --- separator
├── one stat row per changed file
└── aggregate shortstat
```

Metadata appears once, before diff files. Renderer must not repeat commit
metadata inside every file section.

### File summary rows

```text
CHANGELOG.md             | 6 ++++
docs/PLAN_3_DIFF_VIEW.md | 25 ++++++++++++++++-----
src/app.rs               | 50 +++++++++++++++++++++++++++++++++++++++
src/theme.rs             | 121 ++++++++++++++++++++++++++++++++++++++--------
src/ui.rs                | 38 ++++++++++++++++++------
5 files changed, 155 insertions(+), 85 deletions(-)
```

File names align in one column. Addition bars are green, deletion bars red.
Neutral counts stay neutral. Stat rows appear before first file body.

### File marker and separator

```text
Δ CHANGELOG.md
────────────────────────────────────────────────────────────────────────
```

Each changed file gets this marker. Marker uses blue/cyan accent. Separator
uses same blue/cyan accent and spans available pane width. Raw Git headers
remain parser input but do not appear as primary visual file heading:

```text
diff --git a/CHANGELOG.md b/CHANGELOG.md
index 1234567..89abcde 100644
--- a/CHANGELOG.md
+++ b/CHANGELOG.md
```

### Unified body

```text
  9:9   ## Unreleased
 10:10  ### Added
 11:11  - Existing entry
       + New entry
 12:12  - Context line
```

```text
 444:450    self.left_areas[pane] = area;   context: syntax colour, no bg
 445:451    }                              context: syntax colour, no bg
▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
▓   :453    pub fn right_focused(&self) -> bool {  changed block
▓   :456        self.right_focused                  flat changed text
▓   :457    }                                      flat changed text
▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
 449:461    pub fn list_offset(&self, pane: Pane) -> usize  context resumes
```

Rules:

- old:new gutter precedes source text.
- Context lines carry both numbers, syntax foreground, plain background.
- Deletions carry old number only, flat red foreground, red full-width
  background.
- Additions carry new number only, flat green foreground, green full-width
  background.
- Hunk headers are cyan and never syntax-tokenized.
- File markers, metadata, binary notices, and no-newline notices stay flat.
- Changed background fills full pane width, including trailing blank cells.
- Changed background and syntax token colours never coexist.
- Block background stays continuous across consecutive changed lines.

### Gutter and hunk focus

Lazygit keeps line-number state visible even when source text changes:

```text
 193:229  /// Render a parsed Diff for the right pane
 194:230  /// gutter from Diff::line_numbers in front of every line
▓ 196:232  headers, one-sided on an addition/deletion
▓ 197:233  diff highlight on a paired +/- line
▓ 198:234  prefix/suffix background spans full line width
▓ 199:235  word-level highlight stays inside changed block
```

The gutter follows line status:

- context: old and new numbers in neutral/dim colours;
- deletion: old number highlighted red, new side blank;
- addition: old side blank, new number highlighted green;
- hunk/file metadata: blank or marker-only gutter;
- vertical gutter separator remains stable while scrolling.

Focused hunk header appears as a blue outlined row with a bullet:

```text
• 247:  pub fn render_diff(diff: &Diff, focus: Option<&Range<usize>>) -> Text
────────────────────────────────────────────────────────────────────────
```

Focus rules:

- bullet marks current hunk/file anchor;
- blue outline spans pane width;
- focused header remains readable above changed-line backgrounds;
- focus moves with hunk/file navigation;
- scrolling keeps focus state, even when focused header leaves viewport.

### Navigation and focus

```text
]              next hunk in Files view
[              previous hunk in Files view
]              next file in Patch/Commit view
[              previous file in Patch/Commit view
Ctrl-d / Ctrl-u half-page scroll
J / K          line scroll
PageDown/Up    page scroll
```

Focused hunk/file gets blue outlined anchor row plus dim full-block
background. Navigation lands on visible hunk/file marker, not hidden raw
metadata.

## In scope

- `Diff::delta_output(width)`: pipe plain `git diff`/`git show` output to
  `delta --paging=never --line-numbers --width=<pane width>`. Delta owns
  commit metadata, stat rows, `Δ` file markers, separators, line-number
  gutters, word-level highlights, and changed-block backgrounds.
- `theme::render_delta`: parse delta SGR output into ratatui `Text`, preserve
  foreground/background styles, and pad styled rows to pane width so changed
  backgrounds fill the full row. If delta is unavailable or fails, use the
  native renderer fallback.
- Commit right-pane title is `Patch`; commit output gets full pane height so
  delta metadata/stat output is not duplicated by ferrit's own summary row.
- Scroll conversion maps raw Git line offsets to delta's rendered line count;
  scrolling and scrollbar endpoints remain correct when delta adds/removes
  display rows.
- Cache final styled `Text` in `App`, keyed on `right_key`, diff `text`, focus
  range, and pane width. Rebuild only when one changes; pure scroll reuses
  spans.
- Keep `Diff` raw text/ranges unchanged for future patch operations.

## Out of scope

- Staging / unstaging (phase 6).
- User-facing renderer configuration (`delta`, `difftastic`, custom themes).
- Side-by-side layout, combined merge-commit diff, horizontal scroll of
  un-wrapped lines.
- Branches' "Log" body, a stash entry's diff, Status's right side: still mock.

## Definition of done

- `+`/`-` lines: flat colour + full-line background, no per-token syntax
  colour. Context lines: syntax colour, no background.
- `Patch` title, commit metadata, subject, separator, aligned file stat bars,
  aggregate shortstat, blue file markers, blue separators, and one file body
  after another match target structure.
- Gutter colours track context/addition/deletion state; focused anchor has
  bullet and blue full-width outline.
- Scrolling a large diff does not visibly lag; `render_diff` only reruns on a
  diff-content or focus change, not on every redraw.
- `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `cargo machete` clean.
- `CHANGELOG.md` updated.
