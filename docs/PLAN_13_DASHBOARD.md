# Plan: phase 13, repository dashboard

**Status: built (D0 to D5).** `D` opens it. lazygit has no equivalent, so this
phase is not compared with it (`/compare-lazygit` does not apply): it is checked
with the replay harness (`test/scripts/150-dashboard.script`) and frame tests
(`tests/dashboard_screen.rs`, `tests/app_dashboard.rs`, `tests/git_stats.rs`).
Not done, left for a later slice: the `[theme.colors] chart1..chart6` and
`[dashboard] charts` / `hot_files_ignore` config keys (the charts mode is chosen
from the locale and `TERM`, the ignore list is built in), and the real-image donut
(out of scope).

## Goal

One full-screen view, `D`, that answers "what is this repository and what has
been done in it?" without leaving ferrit: totals, activity over time, who did
what, what kind of work it was, which files move most, how healthy the branches
are and what is in progress. Today the pieces are spread out or missing: the
profile drawer lists recent activity and a contributor ranking
(`src/domain/git/activity.rs`, `src/app/screens/profile.rs`), Branches shows
ahead/behind per branch, Status shows the checked-out branch. Nothing gives the
whole picture.

```
┌ Dashboard ─ ferrit ─ main ─────────────────────────────────────────────────── window: 90 days (t) ┐
│ Commits 423 · Authors 2 · Branches 6 (+3 remote) · Tags 5 · 35 commits since v0.7.0                │
├ Activity (commits per day) ───────────────────┬ What was done ─────────────────────────────────────┤
│  58 ┤⡇⠀⠀⠀⠀⣼⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀        │ ⠀⢀⣴⣿⣿⣿⣿⡇⢸⣿⣿⣿⣿⣦⡀⠀  ● feat   29 %  (124)             │
│     │⢱⠀⠀⠀⠀⡇⢇⠀⠀⣰⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀        │ ⢀⠛⠻⢿⡿⠋⠁⠀⠀⠈⠙⢿⣿⣿⣿⡀  ■ docs   24 %  (103)             │
│     │⢸⠀⠀⠀⢠⠃⠸⡀⠀⡟⡆⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡀⠀⠀⢀⣀⣀⠀⡸        │ ⢸⣿⣷⣦⠀⠀⠀⠀⠀⠀⠀⠈⣿⣿⣿⡇  ▲ other  15 %  (64)              │
│   0 ┼⠀⢳⣀⡤⠇⠀⠀⠈⡎⠀⠘⠉⠘⢆⣀⣀⣀⣠⠃⠀⠀⠀⣇⣰⠁⠀⠹⡼        │ ⠈⣉⣤⣶⣶⣄⡀⠀⠀⢀⣠⣾⣿⣿⣿⠀  ◆ fix    13 %  (54)              │
│      09-04                       09-29        │ ⠀⠈⠻⣿⣿⣿⡇⢸⣿⣿⣿⣿⣿⠟⠁⠀  ○ others 19 %  (78)              │
├ Commits per day (6 weeks) ────────────────────┼ Contributors ──────────────────────────────────────┤
│      W1 W2 W3 W4 W5 W6                        │ Richard L.  ██████████████████░░░░░░░ 71 %         │
│ Mon  ·· ·· ░░ ·· ▓▓ ··                        │ Max Wells   ███████░░░░░░░░░░░░░░░░░░ 29 %         │
│ Wed  ·· ·· ▓▓ ·· ··                           │ lines  +87.5k (84 %)  −16.1k (16 %)                │
│ Fri  ·· ██ ██ ▒▒ ▓▓                           │ ████████████████████░░░░                           │
├ Hot files (share of commits touching) ────────┼ Branches ──────────────────────────────────────────┤
│ src/app.rs        ██░░░░░░░░ 20 %             │ 4 active · 1 merged · 0 stale                      │
│ src/ui.rs         ██░░░░░░░░ 20 %             │ feat/read_repo_features  ↑90 ↓299   16 d ago       │
│ src/app/mod.rs    █░░░░░░░░░ 14 %             │ feat/plan_2_backend      ↑23 ↓299   21 d ago       │
│ 2 files hidden (CHANGELOG.md, Cargo.lock)     │ +3 more                                            │
├ In progress ──────────────────────────────────┴────────────────────────────────────────────────────┤
│ 0 changed · 0 stash · main 1 commit ahead of origin                                                │
└────────────────────────────────────────────────────────────────────────────────────────────────────┘
  Esc: back   t: window   n: counts   r: refresh   ?: help          (the key bar, one row under the box)
```

The numbers above are ferrit's own history at the time of writing (computed with
`git`, see "Numbers as shares"); the drawing is a monochrome mock, the real screen
is coloured ("Colours").

Out, on purpose (see "Out of scope"): a per-file or per-commit drill-down,
exporting, charts in a terminal graphics protocol, anything that needs a
network call.

## The gap this fixes

- `activity::commits` walks every local and fetched branch but keeps only the
  last two years and returns raw `CommitEntry` rows; no aggregation exists.
- The profile drawer ranks authors by commit count and nothing else. There is no
  time series, no line counts, no notion of what kind of change a commit was.
- No screen takes the whole terminal: everything is one of the five panes plus
  popups. A dashboard needs the space, so this phase adds the first full-screen
  view.

## Approach

**Headless first.** All numbers come from one function in a new
`src/domain/git/stats.rs`, tested on fixture repositories with no terminal
(principle "backend is headless", `PLAN_0_GENERAL.md`):

```rust
pub struct RepoStats {
    pub window: Window,                 // Days7 | Days30 | Days90 | Year | All
    pub totals: Totals,                 // commits, authors, local/remote branches, tags, stashes, first/last commit
    pub series: Vec<Bucket>,            // commits per day, ISO week or month (by span of history), oldest first
    pub granularity: Granularity,       // Day | Week | Month, chosen from the span ("Charts")
    pub authors: Vec<AuthorStat>,       // grouped by mailmap-resolved email, then merged by name: commits, emails, added, removed, last commit
    pub kinds: Vec<KindStat>,           // feat, fix, docs, test, refactor, perf, style, build, ci, chore, other
    pub hot_files: Vec<FileStat>,       // top 10 by commits touching it, only files that exist in HEAD's tree (share of the window's commits), added, removed; `gone` counts the paths dropped
    pub branches: Vec<BranchHealth>,    // ahead/behind the main branch, age of the tip, merged, current
    pub work: WorkState,                // changed / staged / untracked / conflicted, stashes, ahead/behind upstream
    pub since_tag: Option<TagSince>,    // newest tag reachable from HEAD and the commits since it
    pub sampled: bool,                  // a cap cut the walk short: the screen says so
}
pub(super) fn repo_stats(repo: &Repository, window: Window, cancel: &AtomicBool) -> GitResult<RepoStats>;
```

**Where each number comes from.**

| Numbers | Source | Why |
| --- | --- | --- |
| commits, authors, weekly, kinds, first/last | `git2` revwalk over `refs/heads` and `refs/remotes` (as `activity.rs` does), `author_with_mailmap` | no subprocess, honours `.mailmap` |
| added / removed lines, hot files | `git log --numstat --no-merges --format=%H` through `exec::git` (`src/domain/git/exec.rs`, so it lands in the command log) | `git2` diff per commit is much slower on a big history |
| branch health | `graph_ahead_behind` against the main branch; "merged" is `graph_descendant_of` | no subprocess |
| work in progress | the snapshot ferrit already keeps (`App::files`, header, stash list) | no extra read |

- **Main branch** is `origin/HEAD` if set, else `init.defaultBranch`, else the
  first of `main`, `master` that exists, else HEAD's own branch (then "merged"
  and "stale" are omitted and the panel says why).
- **Kinds** parse the Conventional Commits prefix of the subject (`feat:`,
  `fix(scope):`, `feat!:`); anything else is `other`. Merge commits are not
  counted as a kind. ferrit's own history is written this way, so the panel is
  meaningful here; on a repository that does not, `other` dominates and the panel
  says "no conventional prefixes" instead of a chart of one bar.
- **Hot files** leave out files whose change count says nothing about the code:
  lockfiles (`Cargo.lock`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`,
  `poetry.lock`, `go.sum`, `Gemfile.lock`, `composer.lock`) and changelogs
  (`CHANGELOG*`, `HISTORY*`, `NEWS*`, `RELEASES*`). On ferrit's own history
  `CHANGELOG.md` is otherwise first, touched by nearly every commit. The list is
  the default of a `[dashboard] hot_files_ignore` key in `config.toml`
  (`PLAN_12_POLISH.md` P1), and the panel footer says "N files hidden" when the
  filter removed any, so the omission is never silent.
  Only files that exist in the tree of HEAD are listed (deleted or moved files
  would otherwise rank first, e.g. a `src/app.rs` split into `src/app/`): the
  ranking is computed, the missing paths dropped before the top 10 is taken and
  counted in `gone` (an ignored path counts in `hidden` only; unborn or
  tree-less HEAD skips the check). The log uses `--no-renames`, so a renamed
  file's new path counts only its post-rename commits.
- **Branch order and staleness.** The current branch first, then the branches
  with work not in the main branch, most recently committed to first, then the
  merged ones. A branch is **stale** when it is not the current one and its tip is
  older than 60 days, and it carries a `stale` tag in the row (a word, so colour
  is not the only signal). At most 8 rows show, then "+N more"; the summary line
  counts them ("2 merged, 3 stale"). Ahead and behind are against the main branch.
- **Caps** keep a big repository usable: at most 20 000 commits walked, 5 000
  commits for `--numstat`. When a cap is hit, `sampled = true` and the screen
  shows "sampled: newest 5 000 commits".
- **Weeks** are ISO weeks in UTC so the same repository gives the same chart on
  every machine and in tests.
- **Period covered.** The time axis spans the window clipped to the repository's
  own age (first commit to now), and its label says what is drawn: a four-week-old
  repository under a 90-day window reads "4 weeks", not "12 weeks", and the chart
  uses the whole width for those four weeks instead of leaving eight blank ones.
  With fewer than two buckets there is nothing to draw: the counts are shown as
  text ("3 commits this week").
- **Relative times** come from one formatter used everywhere on the screen: under
  a minute "just now", then "5 min ago", "2 h ago", "3 d ago", "6 w ago",
  "4 mo ago", "2 y ago". Never "0 h ago".

**Numbers as shares.** Wherever the screen compares parts of a whole, the
percentage is the primary figure and the count is secondary (dimmed, in
brackets): kinds of change (`feat 41 %  (124)`), contributors' share of commits,
lines added versus removed, branches by state (current, active, merged, stale),
the share of the window's commits per week. Rules, so the numbers stay honest:

- Shares are computed over the **whole** in the window, before any cut to "top
  N" lists, so a top-3 that covers 92 % says 92 %, not 100 %.
- Rounding uses the largest-remainder method so the displayed shares of one
  chart add to exactly 100 %; a share under 1 % reads `<1 %`, never `0 %` for a
  non-empty group.
- A total of zero shows `–`, never a percentage of nothing.
- **A share needs a whole big enough to mean something.** Under 20 items in the
  whole (six branches, three tags) the dashboard shows counts ("4 active, 1
  merged"), not "17 %". A percentage of a handful misleads.
- **Hot files are a share of commits, not of touches**: "20 %" means one commit in
  five touched the file. As a share of all file touches the first file of ferrit's
  own history is 4.6 %, because hundreds of files split the total, and the figure
  says nothing.
- Totals that are not parts of a whole (commits, authors, tags) stay counts.
- `n` on the screen swaps the primary figure to counts for people who prefer them;
  the choice is kept for the session.

## Charts

Every visual is a ratatui widget already in the dependency tree (`ratatui` 0.30
with its default widgets; `Cargo.toml` needs no new dependency), except the donut,
which is a custom `Shape` drawn on ratatui's `Canvas`.

| Visual | Where | Built with | Detail |
| --- | --- | --- | --- |
| **Line chart**, commits over time | Activity | `Chart` with a `Dataset` in `GraphType::Line`, `Marker::Braille` | y axis labelled 0 and the peak, x axis first and last date |
| **Donut**, kinds of change | What was done | `Canvas` + a `Donut` `Shape` (`src/components/ui/donut.rs`), `Marker::Braille` | up to 6 slices, legend on the right with `%` first |
| **Share bars**, 100 % | Contributors, lines added / removed, Hot files | `LineGauge`, or a drawn `█░` bar where the widget does not fit | one bar per row, the percentage right-aligned |
| **Heat map**, commits per day | Commits per day | own widget (`src/components/ui/heatmap.rs`), 2 characters per day | weeks in columns; five levels `· ░ ▒ ▓ █` |

**Line chart granularity** follows the span of history so the chart never has 4
points across the whole width: daily up to 60 days of history, weekly up to two
years, monthly beyond. The axis label says which ("commits per day"). Fewer than 2
points: text instead of a chart.

**Donut.**
- Angles come from the shares of the whole window, largest first, starting at 12
  o'clock, clockwise. Slices under 3 % are merged into "others"; at most 6 slices
  are drawn.
- A terminal cell is about twice as tall as wide, so the ring is sampled on a
  32 × 32 dot grid over 16 columns × 8 rows to look round. Minimum size 12 × 6
  cells; below that the donut is replaced by one 100 % share bar.
- One slice draws a full ring at 100 %. A zero total draws no ring and says
  "no commits in this window".
- A Braille cell has one foreground colour, so the border between two slices is
  resolved to a cell: a one-dot gap keeps them apart, and the legend carries the
  exact figures.

**Heat map.**
- 7 rows (Mon to Sun) when the panel is 9 rows or taller, else 3 rows (Mon, Wed,
  Fri). Weeks shown: as many as fit, capped at 26.
- Levels come from the quantiles of the non-zero days, not from the maximum, so one
  burst day does not flatten the rest to the palest shade.
- Days after today are blank, days with no commit show `·`.

**Fallback.** `[dashboard] charts = "auto" | "braille" | "blocks"` in `config.toml`.
`auto` chooses `braille` unless the locale is not UTF-8 or `TERM=linux` (the Linux
console has no Braille glyphs), then `blocks`: the line chart becomes a
`Sparkline` of block glyphs, the donut a stacked 100 % bar, the heat map is
unchanged (it already uses block characters).

## Colours

Colours come from the theme, never hard-coded in the widgets. `Palette`
(`src/components/ui/palette.rs`) already carries the semantic colours and the
`[theme]` accent (green, blue, purple, amber) and `[theme.colors]` overrides of
`PLAN_12_POLISH.md` P5; the dashboard adds a small `ChartPalette` derived from it,
so a theme change recolours the charts too.

**One meaning, one colour, on every visual.** A kind of change or an author keeps
the same colour in the donut, the legend and the bars.

| Use | Colour | Why |
| --- | --- | --- |
| line chart series, single-series bars (Hot files) | the theme accent (`Palette.focus`) | one series, one colour, no rainbow |
| lines added / removed | `Palette.add` / `Palette.del` | the same green and red as the diff |
| kinds of change | `feat` green, `fix` yellow, `docs` blue, `test` cyan, `refactor` magenta, all the rest "others" gray | six named ANSI colours, so the terminal's own theme decides the exact shade |
| authors | the same six colours in order of commits, the sixth and beyond "others" gray | stable for the session |
| heat map | the accent, from the dimmest to its bright variant, over the glyph density `░▒▓█` | the glyphs carry the level, the colour reinforces it |
| branch state | current: accent, bold · active: default · merged: gray · stale: `Palette.warn` | stale is the only alert |
| ahead / behind arrows | as Branches: `↑` `Palette.warn` | one convention across the app |

**Colour is never the only signal.** Donut slices carry a shape marker in the
legend (`● ■ ▲ ◆ ○`), every bar and slice carries its percentage, the heat map
uses glyph density, and `stale` is written out.

**Light terminals.** `Palette.light` selects darker variants for the colours that
wash out on white (yellow, cyan): `ChartPalette::for(&Palette)` returns explicit
RGB values in that case, ANSI names otherwise. ferrit does not know the terminal's
background colour, so the ANSI names are the safe default and the RGB variants are
opt-in through `[theme] base = "light"`.

**Overrides.** `[theme.colors]` accepts `chart1` to `chart6` (`"#rrggbb"` or an
ANSI name) for the six categorical colours; unset ones keep the defaults above.

**Off the UI thread.** Same shape as the remote operations
(`docs/PLAN_9_REMOTE.md`, "Approach part 2"): a worker computes `RepoStats` and
sends `AppEvent::StatsDone { generation, stats }`; a stale generation is dropped
(as `DiffDone` does); closing the screen or changing the window sets a cancel
flag the walk polls. The screen shows "computing…" and fills in as soon as the
cheap part (revwalk) is done; the churn columns (`+`/`−`, hot files) arrive in a
second event, so the first paint is fast.

**Cache.** The last `RepoStats` per window is kept until the next refresh event
changes HEAD or a ref (`AppEvent::Refresh` already fires on that); reopening the
screen is then instant. `r` recomputes.

## Backend: `src/domain/git/stats.rs`

New module next to `activity.rs`; `activity::commits` stays as it is (the profile
drawer uses it). Share the ref-walk setup rather than copying it: extract the
"push every local and remote branch tip" loop into a helper both call.
`Repo::stats(window, cancel)` is the only public entry, like the other `Repo`
methods (`src/domain/git/mod.rs`). Nothing here imports `ratatui`.

## App wiring

- `src/app/dashboard.rs` (new, ferrit state and events, not drawing): the open
  flag, the `Window`, the cached `RepoStats` per window, the generation counter,
  the scroll offset, the worker handle. `App::open_dashboard`,
  `on_stats_done`, `dashboard_key`.
- **Full-screen mode.** Add a small `FullScreen` enum (`None`, `Dashboard`) to
  `App`; `ui::draw` (`src/app/screens/mod.rs`) draws only the full-screen view
  when it is set. It is the state `PLAN_14_GIT_CONFIG.md` reuses for its editor:
  whichever phase lands first adds the enum, the other extends it.
- While it is up, input goes to `dashboard_key` first, then the always-on globals
  (`q` quits, `?` help). It must not run pane actions behind the screen.

## Rendering: `src/app/screens/dashboard.rs`

Pure function of `(&RepoStats, area, palette)`; the widgets are generic and live
in `src/components/ui/` (`donut.rs`, `heatmap.rs`, `share_bar.rs`; the line chart and
gauge are ratatui's own) so a later screen can reuse
them and they can be tested alone.

- **Wide (≥ 110 columns):** two columns as in the diagram. **Narrow (60 to 109):**
  one column, sections stacked, scrolling with `j`/`k`. **Under 60 columns:**
  totals and the in-progress line only, with "widen the terminal for charts".
- Bars: `█` and `░`, width from the available columns, the percentage
  right-aligned and the count dimmed after it; `n` swaps them. The block
  `Sparkline` (`▁▂▃▄▅▆▇█`) is only the fallback of the line chart ("Charts").
- Colours: see "Colours". No colour carries meaning alone (the word "stale" and the
  legend markers carry it too).
- Long paths are cut in the middle (`src/app/…/mod.rs`), never wrapped.
- Built as (D3b): the page is drawn on an off-screen buffer as tall as its
  content, then the rows `scroll` selects are copied (every layout scrolls the
  same way, and the renderer hands the largest useful offset back to the app,
  which clamps to it). The wide layout shrinks the bands towards their minimum
  before it scrolls. Kinds other than the five named ones are one gray "others"
  slice. A branch is counted and coloured `stale` whenever the domain says so,
  merged or not; `merged` counts the merged branches that are not stale.

## Keybindings (new)

| Key | Where | Action |
| --- | --- | --- |
| `D` | global | open the dashboard (rebindable, `Action::Dashboard`, `PLAN_12_POLISH.md` P2) |
| `Esc`, `q`, `D` | dashboard | close it |
| `t` / `T` | dashboard | next / previous window: 7d, 30d, 90d, 1y, all |
| `n` | dashboard | swap the primary figure between percentages and counts |
| `r` | dashboard | recompute |
| `j` `k` `PgUp` `PgDn` `Home` `End` | dashboard | scroll when the content is taller than the screen |

`D` is lazygit's Reset key. ferrit has no Reset, so there is no clash today; if a
Reset action is added later, this key moves (it is in the keymap, not hard-coded).
The key bar shows `Back: esc | Window: t | Counts: n | Refresh: r | Help: ?` (`Bar::Dashboard`,
same mechanism as `Bar::Help`).

## Edge cases

| Case | Behaviour |
| --- | --- |
| empty repository, unborn HEAD | totals read 0; charts replaced by "no commits yet" |
| shallow clone | banner "shallow: history before <date> is not available" |
| detached HEAD | works; "main branch" resolved as above, current shown as `(detached)` |
| no remote | remote counts hidden, not shown as 0 |
| window with no commits | charts empty with "no commits in this window", totals unchanged |
| huge history | caps above; `sampled` notice; the screen is usable before the churn arrives |
| author with several emails | grouped through `.mailmap` first; then the entries that share a name (case-insensitive, trimmed, non-empty) merge into one row: commits and lines summed, the most frequent email names it, `emails` lists all ("3 emails"). Two people who share a name are merged too: a deliberate choice |
| bots, empty names | shown as is; nothing is filtered |
| `git log` missing or failing | churn columns show `n/a`, the rest still renders; the error goes to the command log |
| terminal resized while open | redraw; sections reflow |
| terminal without Braille (`TERM=linux`, non-UTF-8 locale) | `charts = "auto"` falls back to blocks; nothing shows as `?` boxes |
| one author, or one kind at 100 % | full ring / single bar at 100 %, legend of one row |
| donut area under 12 × 6 cells | replaced by one 100 % share bar |
| more than 6 authors or kinds | the top 5 and a gray "others" |
| history shorter than two buckets | text instead of a line chart |

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/git_stats.rs`: builds fixtures with several authors, dates
  (`GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE`) and conventional prefixes, and asserts
  totals, weekly buckets, kinds, author grouping with a `.mailmap`, the caps
  (`sampled`), branch health, and the empty / shallow / detached cases.
- Unit tests for the widgets (`sparkline` scaling, `bars` widths) and for the
  Conventional Commits parser (`feat!:`, scopes, `Revert "…"`, `Merge …`).
- `test/scripts/150-dashboard.script`: open with `D`, `expect-text` on the
  totals and a known author, `t` changes the window label, `Esc` returns to the
  panes, and a `snapshot` of the wide and narrow layouts.
- Widget tests on a `TestBackend` buffer: the donut's slice angles and that the
  displayed shares add to 100 (largest remainder), one slice = full ring, an empty
  total draws nothing, adjacent slices never share a colour; heat map levels from
  quantiles and blank future days; the line chart's granularity choice (26 days ->
  daily, 200 days -> weekly, 5 years -> monthly); a `<3 %` slice folds into
  "others".
- Colour tests read the `fg` of cells in the rendered buffer: the same kind has the
  same colour in the donut and in the legend, `stale` uses `Palette.warn`, and
  `charts = "blocks"` draws no Braille character.
- A stale-generation test: two windows requested quickly, only the last paints.

## Milestones

- **D0** `stats.rs` totals, weekly, authors, kinds, branches, work, with tests.
- **D1** numstat churn and hot files, caps and `sampled`.
- **D2** `FullScreen`, `dashboard.rs` state, worker, `AppEvent::StatsDone`,
  cache and cancel.
- **D3a** widgets: `donut.rs`, `heatmap.rs`, `share_bar.rs`, each with tests; the
  line chart wired on `Chart`.
- **D3b** `screens/dashboard.rs`: layout at three widths, percentages first,
  `n` to swap to counts.
- **D3c** `ChartPalette`, `chart1..6`, light variants, `[dashboard] charts`.
- **D4** keymap entry, key bar, help text, `t`/`r`/scroll.
- **D5** replay script, snapshots, README row, CHANGELOG line.

## Definition of done (phase 13)

`D` opens the dashboard on a real repository in under a second on ferrit's own
history and stays responsive on a 100 000-commit one (the first paint does not
wait for the churn). Every number on it can be reproduced by a documented `git`
command. `Esc` leaves nothing changed in the panes (selection, scroll, focus).
`cargo clippy --all-targets --all-features -- -D warnings` and `cargo test` are
green, the replay script passes, and the README table marks the row ✅.
Every comparison on the screen reads as a percentage first (counts on request with
`n`), every chart has a non-Braille fallback, and no meaning depends on colour alone.

## Out of scope

- drilling into an author, a file or a branch from the dashboard (later: Enter
  could filter Commits by author)
- exporting to Markdown, JSON or an image
- comparing two repositories, or two time ranges side by side
- anything that calls the network (issue counts, CI status, stars)
- real images (a smooth donut through `ratatui-image` on Kitty, iTerm2 or Sixel
  terminals): a second render path to maintain for a gain on some terminals only;
  the Braille donut is the one path
- a sixth permanent pane
