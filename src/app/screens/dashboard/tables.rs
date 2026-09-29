//! The row-based sections of the dashboard: Contributors, Hot files,
//! Branches, In progress, and the totals line. Every row is cut to the width,
//! never wrapped.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::charts::NO_COMMITS;
use super::text::{
    MIN_WHOLE, compact, cut_end, cut_middle, figure, figure_columns, plural, relative_time,
};
use super::{Ctx, note};
use crate::components::ui::chart_palette::OTHERS;
use crate::components::ui::share_bar::{percent_label, single_bar, stacked_bar};
use crate::domain::git::stats::branches::BranchHealth;
use crate::domain::git::stats::share::{Share, fold, shares};

/// Branch rows drawn before "+N more".
pub(super) const BRANCH_ROWS: usize = 8;
/// Hot files listed at most (the stats keep ten).
const HOT_ROWS: usize = 10;
/// Hidden files named in the footer before "…".
const HIDDEN_NAMED: usize = 3;
/// Narrowest bar worth drawing.
const MIN_BAR: usize = 3;

fn text_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

/// `name  bar  figure` rows: the columns are sized to the widest name (capped)
/// and figure, and the bar takes the rest.
struct BarRow {
    name: String,
    name_style: Style,
    fraction: f64,
    color: ratatui::style::Color,
    figure: String,
}

fn bar_rows(ctx: &Ctx<'_>, width: u16, rows: &[BarRow], name_cap: usize) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let figures: Vec<String> = rows.iter().map(|r| r.figure.clone()).collect();
    let (columns, fig_w) = figure_columns(&figures, ctx.dim());
    let name_w = rows
        .iter()
        .map(|r| text_width(&r.name))
        .max()
        .unwrap_or(0)
        .min(name_cap)
        .min(width.saturating_sub(fig_w + 2 + MIN_BAR + 1).max(4));
    let bar_w = width.saturating_sub(name_w + fig_w + 2);
    rows.iter()
        .zip(columns)
        .map(|(r, figure)| {
            let name = cut_middle(&r.name, name_w);
            let pad = " ".repeat(name_w.saturating_sub(text_width(&name)));
            let mut spans = vec![Span::styled(format!("{name}{pad} "), r.name_style)];
            if bar_w >= MIN_BAR {
                let bar_w = u16::try_from(bar_w).unwrap_or(u16::MAX);
                spans.extend(single_bar(r.fraction, bar_w, r.color));
                spans.push(Span::raw(" "));
            }
            spans.extend(figure);
            Line::from(spans)
        })
        .collect()
}

pub(super) fn contributors(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let stats = ctx.stats;
    let total: u64 = stats.authors.iter().map(|a| count(a.commits)).sum();
    if total == 0 {
        note(buf, area, NO_COMMITS, ctx.dim());
        return;
    }
    let items = stats
        .authors
        .iter()
        .enumerate()
        .map(|(i, a)| (i, count(a.commits)))
        .collect();
    let folded = fold(items, 0, 6);
    let split = folded.shares();
    let colors = ctx.colors();
    let mut rows: Vec<BarRow> = Vec::new();
    let mut counts: Vec<(String, u64)> = folded
        .slices
        .iter()
        .map(|&(i, n)| {
            let name = stats.authors.get(i).map_or("?", |a| a.name.as_str());
            (name.to_owned(), n)
        })
        .collect();
    if folded.others > 0 {
        counts.push(("others".to_owned(), folded.others));
    }
    for (rank, ((name, n), share)) in counts.iter().zip(&split.items).enumerate() {
        let rank = if folded.others > 0 && rank + 1 == counts.len() {
            OTHERS
        } else {
            rank
        };
        rows.push(BarRow {
            name: name.clone(),
            name_style: Style::new(),
            fraction: fraction(*n, split.total),
            color: colors.author_color(rank),
            figure: figure(*share, split.total, ctx.view.show_counts),
        });
    }
    let mut lines = bar_rows(ctx, area.width, &rows, 16);
    lines.extend(lines_summary(ctx, area.width));
    Paragraph::new(lines).render(area, buf);
}

#[allow(
    clippy::cast_precision_loss,
    reason = "commit counts are far below 2^53"
)]
fn fraction(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

/// The `lines +84 % (87.5k) −16 % (16.1k)` line and its stacked bar.
fn lines_summary(ctx: &Ctx<'_>, width: u16) -> Vec<Line<'static>> {
    let dim = ctx.dim();
    let label =
        |text: String| Line::from(vec![Span::styled("lines  ", dim), Span::styled(text, dim)]);
    let Some(lines) = ctx.stats.totals.lines else {
        let why = if ctx.view.churn_pending {
            "computing…"
        } else {
            "n/a"
        };
        return vec![label(why.to_owned())];
    };
    let split = shares(&[lines.added, lines.removed]);
    if split.total == 0 {
        return vec![label("–".to_owned())];
    }
    let colors = ctx.colors();
    let part = |sign: &str, share: &Share, n: u64| -> String {
        if split.total < MIN_WHOLE {
            return format!("{sign}{}", compact(n));
        }
        let counts = ctx.view.show_counts;
        let percent = if share.under_one {
            "<1 %".to_owned()
        } else {
            share
                .percent
                .map_or_else(|| "–".to_owned(), |p| format!("{p} %"))
        };
        if counts {
            format!("{sign}{}  ({percent})", compact(n))
        } else {
            format!("{sign}{percent}  ({})", compact(n))
        }
    };
    let (Some(added), Some(removed)) = (split.items.first(), split.items.get(1)) else {
        return Vec::new();
    };
    let line = Line::from(vec![
        Span::styled("lines  ", dim),
        Span::styled(part("+", added, lines.added), Style::new().fg(colors.add)),
        Span::raw("   "),
        Span::styled(
            part("−", removed, lines.removed),
            Style::new().fg(colors.del),
        ),
    ]);
    vec![
        line,
        stacked_bar(
            &[(lines.added, colors.add), (lines.removed, colors.del)],
            width,
        ),
    ]
}

pub(super) fn hot_title(ctx: &Ctx<'_>) -> &'static str {
    match &ctx.stats.hot_files {
        Some(hot) if count(hot.commits) < MIN_WHOLE => "Hot files (commits touching)",
        _ => "Hot files (share of commits touching)",
    }
}

pub(super) fn hot_files(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    if ctx.stats.totals.commits == 0 {
        note(buf, area, NO_COMMITS, ctx.dim());
        return;
    }
    let Some(hot) = &ctx.stats.hot_files else {
        let why = if ctx.view.churn_pending {
            "computing…"
        } else {
            "n/a"
        };
        note(buf, area, why, ctx.dim());
        return;
    };
    if hot.files.is_empty() {
        note(buf, area, "no file changes in this window", ctx.dim());
        return;
    }
    let whole = count(hot.commits);
    let rows: Vec<BarRow> = hot
        .files
        .iter()
        .take(HOT_ROWS)
        .map(|f| BarRow {
            name: f.path.clone(),
            name_style: Style::new(),
            fraction: fraction(f.share.count, whole),
            color: ctx.colors().accent,
            figure: figure(f.share, whole, ctx.view.show_counts),
        })
        .collect();
    let mut lines = bar_rows(ctx, area.width, &rows, 28);
    if !hot.hidden.is_empty() {
        let named: Vec<&str> = hot
            .hidden
            .iter()
            .take(HIDDEN_NAMED)
            .map(String::as_str)
            .collect();
        let more = if hot.hidden.len() > HIDDEN_NAMED {
            ", …"
        } else {
            ""
        };
        let text = format!(
            "{} hidden ({}{more})",
            plural(hot.hidden.len(), "file", "files"),
            named.join(", ")
        );
        lines.push(Line::styled(
            cut_end(&text, usize::from(area.width)),
            ctx.dim(),
        ));
    }
    Paragraph::new(lines).render(area, buf);
}

/// How a branch row is styled and counted. `stale` is the domain's flag (not
/// current, tip older than 60 days); a stale branch is the alert whether or not
/// it is merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Stale,
    Merged,
    Active,
}

fn state(b: &BranchHealth) -> State {
    if b.stale {
        State::Stale
    } else if !b.current && b.vs_main.is_some_and(|v| v.merged) {
        State::Merged
    } else {
        State::Active
    }
}

fn branch_summary(ctx: &Ctx<'_>) -> Line<'static> {
    let branches = &ctx.stats.branches;
    let dim = ctx.dim();
    if ctx.stats.main_branch.is_none() {
        let text = format!(
            "{}: no main branch found, merged and stale are not shown",
            plural(branches.len(), "branch", "branches")
        );
        return Line::styled(text, dim);
    }
    let of = |wanted: State| branches.iter().filter(|b| state(b) == wanted).count();
    let (active, merged, stale) = (of(State::Active), of(State::Merged), of(State::Stale));
    let split = shares(&[count(active), count(merged), count(stale)]);
    let part = |n: usize, share: Option<&Share>, word: &str| -> String {
        match share {
            Some(s) if split.total >= MIN_WHOLE => {
                let percent = if s.under_one { None } else { s.percent };
                format!(
                    "{} {word}",
                    percent_label(percent, count(n), ctx.view.show_counts)
                )
            },
            _ => format!("{n} {word}"),
        }
    };
    let mut spans = vec![
        Span::raw(part(active, split.items.first(), "active")),
        Span::styled(" · ", dim),
        Span::styled(
            part(merged, split.items.get(1), "merged"),
            ctx.colors().branch_merged,
        ),
        Span::styled(" · ", dim),
    ];
    let stale_style = if stale > 0 {
        ctx.colors().branch_stale
    } else {
        Style::new()
    };
    spans.push(Span::styled(
        part(stale, split.items.get(2), "stale"),
        stale_style,
    ));
    Line::from(spans)
}

pub(super) fn branches(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let stats = ctx.stats;
    let colors = ctx.colors();
    let dim = ctx.dim();
    let total = stats.branches.len();
    // The summary and, at the bottom, "+N more"; the rows in between.
    let room = usize::from(area.height).saturating_sub(1);
    let shown = if total <= BRANCH_ROWS && total <= room {
        total
    } else {
        total.min(BRANCH_ROWS).min(room.saturating_sub(1))
    };
    let hidden = total - shown;
    let arrows: Vec<String> = stats
        .branches
        .iter()
        .take(shown)
        .map(|b| {
            b.vs_main
                .map_or_else(String::new, |v| format!("↑{} ↓{}", v.ahead, v.behind))
        })
        .collect();
    let ages: Vec<String> = stats
        .branches
        .iter()
        .take(shown)
        .map(|b| relative_time(ctx.view.now, b.tip_time))
        .collect();
    let arrow_w = arrows.iter().map(|a| text_width(a)).max().unwrap_or(0);
    let age_w = ages.iter().map(|a| text_width(a)).max().unwrap_or(0);
    let any_stale = stats
        .branches
        .iter()
        .take(shown)
        .any(|b| state(b) == State::Stale);
    let stale_w = if any_stale { 6 } else { 0 };
    let fixed = 2 + 2 + arrow_w + 2 + age_w + stale_w;
    let name_w = stats
        .branches
        .iter()
        .take(shown)
        .map(|b| text_width(&b.name))
        .max()
        .unwrap_or(0)
        .min(usize::from(area.width).saturating_sub(fixed).max(6));
    let mut lines = vec![branch_summary(ctx)];
    for ((b, arrow), age) in stats.branches.iter().zip(&arrows).zip(&ages).take(shown) {
        let st = state(b);
        let style = match (b.current, st) {
            (_, State::Stale) => colors.branch_stale,
            (true, _) => colors.branch_current,
            (false, State::Merged) => colors.branch_merged,
            (false, State::Active) => colors.branch_active,
        };
        let name = cut_end(&b.name, name_w);
        let pad = " ".repeat(name_w.saturating_sub(text_width(&name)));
        let mut spans = vec![
            Span::styled(if b.current { "* " } else { "  " }, style),
            Span::styled(format!("{name}{pad}  "), style),
        ];
        let (up, down) = arrow.split_once(' ').unwrap_or((arrow.as_str(), ""));
        let arrow_pad = " ".repeat(arrow_w.saturating_sub(text_width(arrow)));
        spans.push(Span::styled(up.to_owned(), Style::new().fg(colors.warn)));
        spans.push(Span::raw(if down.is_empty() {
            String::new()
        } else {
            format!(" {down}")
        }));
        spans.push(Span::raw(format!("{arrow_pad}  ")));
        spans.push(Span::styled(format!("{age:>age_w$}"), dim));
        if st == State::Stale {
            spans.push(Span::styled(
                " stale",
                colors.branch_stale.add_modifier(Modifier::BOLD),
            ));
        }
        lines.push(Line::from(spans));
    }
    if hidden > 0 {
        lines.push(Line::styled(format!("+{hidden} more"), dim));
    }
    Paragraph::new(lines).render(area, buf);
}

/// The single line of the In progress section.
pub(super) fn progress_line(ctx: &Ctx<'_>) -> Line<'static> {
    let work = &ctx.stats.work;
    let mut parts = vec![format!("{} changed", work.changed)];
    for (n, word) in [
        (work.staged, "staged"),
        (work.untracked, "untracked"),
        (work.conflicted, "conflicted"),
    ] {
        if n > 0 {
            parts.push(format!("{n} {word}"));
        }
    }
    parts.push(format!("{} stash", work.stashes));
    let branch = ctx.view.branch;
    match &work.upstream {
        Some(upstream) if work.ahead > 0 => parts.push(format!(
            "{branch} {} ahead of {upstream}",
            plural(work.ahead, "commit", "commits")
        )),
        Some(upstream) if work.behind > 0 => parts.push(format!(
            "{branch} {} behind {upstream}",
            plural(work.behind, "commit", "commits")
        )),
        Some(upstream) => parts.push(format!("{branch} up to date with {upstream}")),
        None => parts.push(format!("{branch} has no upstream")),
    }
    let style = if work.conflicted > 0 {
        Style::new().fg(ctx.colors().warn)
    } else {
        Style::new()
    };
    Line::styled(parts.join(" · "), style)
}

/// `Commits 423 · Authors 2 · Branches 6 (+3 remote) · Tags 5 · 35 commits since v0.7.0`.
pub(super) fn totals_line(ctx: &Ctx<'_>) -> Line<'static> {
    let t = &ctx.stats.totals;
    let dim = ctx.dim();
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let sep = || Span::styled(" · ", dim);
    let item = |label: &str, value: String| {
        vec![
            Span::styled(format!("{label} "), dim),
            Span::styled(value, bold),
        ]
    };
    let mut spans = item("Commits", t.commits.to_string());
    spans.push(sep());
    spans.extend(item("Authors", t.authors.to_string()));
    spans.push(sep());
    let remote = if t.remote_branches > 0 {
        format!(" (+{} remote)", t.remote_branches)
    } else {
        String::new()
    };
    spans.extend(item("Branches", format!("{}{remote}", t.local_branches)));
    spans.push(sep());
    spans.extend(item("Tags", t.tags.to_string()));
    if let Some(since) = &ctx.stats.since_tag {
        spans.push(sep());
        spans.push(Span::raw(format!(
            "{} since {}",
            plural(since.commits, "commit", "commits"),
            since.name
        )));
    }
    Line::from(spans)
}
