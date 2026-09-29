# Plan: phase 13, repository dashboard

**Status: planned.** Nothing here exists yet. lazygit has no equivalent, so this
phase is not compared with it (`/compare-lazygit` does not apply): it is checked
with the replay harness and screenshots (`PLAN_SELF_TESTING.md`).

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
┌ Dashboard ─ ferrit ─ main ─────────────────────── window: 90 days (t) ┐
│ Commits 1 204    Authors 3    Branches 8 (+5 remote)    Tags 7        │
│ First 2026-08-30 · last 2 h ago · since v0.7.0: 34 commits            │
├ Activity (per week) ──────────────────┬ What was done ────────────────┤
│ ▁▂▃▅▇▆▅▃▂▃▅▇  peak 214 (wk of 09-22)  │ feat      ████████░░  41      │
│ 12 weeks · 812 commits                │ fix       ██████░░░░  29      │
├ Contributors ─────────────────────────┤ docs      ███░░░░░░░  12      │
│ Max Wells     ███████████ 612  +18k −6k│ chore     ██░░░░░░░░   8      │
│ Richard L.    ██████      401  +9k −3k│ test      █░░░░░░░░░   5      │
├ Hot files (touches) ──────────────────┼ Branches ─────────────────────┤
│ src/app/mod.rs           86  +2.1k −1k│ main          ↑0 ↓0    now    │
│ CHANGELOG.md             71           │ feat/branches ↑3 ↓12   3 w  ⚠ │
│ tests/lazygit_parity.rs  44           │ 2 merged into main, 1 stale   │
├ In progress ──────────────────────────┴───────────────────────────────┤
│ 3 changed · 1 staged · 1 untracked · 1 stash · main ↑0 ↓0 of origin   │
└ Esc: back   t: window   r: refresh   j/k: scroll ─────────────────────┘
```

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
    pub weekly: Vec<WeekBucket>,        // commits per ISO week in the window, oldest first
    pub authors: Vec<AuthorStat>,       // grouped by mailmap-resolved email: commits, added, removed, last commit
    pub kinds: Vec<KindStat>,           // feat, fix, docs, test, refactor, perf, style, build, ci, chore, other
    pub hot_files: Vec<FileStat>,       // top 10 by number of commits touching it: touches, added, removed
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
- **Caps** keep a big repository usable: at most 20 000 commits walked, 5 000
  commits for `--numstat`. When a cap is hit, `sampled = true` and the screen
  shows "sampled: newest 5 000 commits".
- **Weeks** are ISO weeks in UTC so the same repository gives the same chart on
  every machine and in tests.

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
in `src/components/ui/` (`sparkline.rs`, `bars.rs`) so a later screen can reuse
them and they can be tested alone.

- **Wide (≥ 110 columns):** two columns as in the diagram. **Narrow (60 to 109):**
  one column, sections stacked, scrolling with `j`/`k`. **Under 60 columns:**
  totals and the in-progress line only, with "widen the terminal for charts".
- Sparkline: `▁▂▃▄▅▆▇█`, scaled to the window's peak, the peak labelled. Bars: `█`
  and `░`, width from the available columns, the number right-aligned.
- Colours follow the palette (`Palette.add` for `+`, `Palette.del` for `−`,
  `warn` for stale branches). No colour carries meaning alone (the `⚠` glyph and
  the word "stale" also do).
- Long paths are cut in the middle (`src/app/…/mod.rs`), never wrapped.

## Keybindings (new)

| Key | Where | Action |
| --- | --- | --- |
| `D` | global | open the dashboard (rebindable, `Action::Dashboard`, `PLAN_12_POLISH.md` P2) |
| `Esc`, `q`, `D` | dashboard | close it |
| `t` / `T` | dashboard | next / previous window: 7d, 30d, 90d, 1y, all |
| `r` | dashboard | recompute |
| `j` `k` `PgUp` `PgDn` `Home` `End` | dashboard | scroll when the content is taller than the screen |

`D` is lazygit's Reset key. ferrit has no Reset, so there is no clash today; if a
Reset action is added later, this key moves (it is in the keymap, not hard-coded).
The key bar shows `Back: esc | Window: t | Refresh: r | Help: ?` (`Bar::Dashboard`,
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
| author with several emails | grouped through `.mailmap` when present; otherwise listed apart (documented, not guessed) |
| bots, empty names | shown as is; nothing is filtered |
| `git log` missing or failing | churn columns show `n/a`, the rest still renders; the error goes to the command log |
| terminal resized while open | redraw; sections reflow |

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
- A stale-generation test: two windows requested quickly, only the last paints.

## Milestones

- **D0** `stats.rs` totals, weekly, authors, kinds, branches, work, with tests.
- **D1** numstat churn and hot files, caps and `sampled`.
- **D2** `FullScreen`, `dashboard.rs` state, worker, `AppEvent::StatsDone`,
  cache and cancel.
- **D3** `screens/dashboard.rs` and the two widgets, three widths.
- **D4** keymap entry, key bar, help text, `t`/`r`/scroll.
- **D5** replay script, snapshots, README row, CHANGELOG line.

## Definition of done (phase 13)

`D` opens the dashboard on a real repository in under a second on ferrit's own
history and stays responsive on a 100 000-commit one (the first paint does not
wait for the churn). Every number on it can be reproduced by a documented `git`
command. `Esc` leaves nothing changed in the panes (selection, scroll, focus).
`cargo clippy --all-targets --all-features -- -D warnings` and `cargo test` are
green, the replay script passes, and the README table marks the row ✅.

## Out of scope

- drilling into an author, a file or a branch from the dashboard (later: Enter
  could filter Commits by author)
- exporting to Markdown, JSON or an image
- comparing two repositories, or two time ranges side by side
- anything that calls the network (issue counts, CI status, stars)
- a sixth permanent pane
