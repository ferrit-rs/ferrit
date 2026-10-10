//! The row-based sections of the dashboard: Contributors, Hot files,
//! Branches, the stat tiles, the work in progress and the compact totals line. Every row is cut to the width,
//! never wrapped.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::git::stats::HotFiles;
use crate::git::stats::branch_health::BranchHealth;
use crate::git::stats::share::{Share, fold, shares};
use crate::tui::components::dashboard::charts::NO_COMMITS;
use crate::tui::components::dashboard::text::{
    MIN_WHOLE, compact, figure, figure_columns, plural, relative_time,
};
use crate::tui::components::dashboard::view::{Ctx, note};
use crate::tui::widgets::chrome::text::{cut_end, cut_middle};
use crate::tui::widgets::share_bar::{percent_label, single_bar, stacked_bar};

/// Branch rows drawn before "+N more".
pub(crate) const BRANCH_ROWS: usize = 8;
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
/// and figure, and the bar takes the rest. Text is ink or dim, only the bar
/// wears `color`.
struct BarRow {
    name: String,
    name_style: Style,
    /// Dim words after the name (`(3 emails)`), never cut.
    note: String,
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
        .map(|r| text_width(&r.name) + text_width(&r.note))
        .max()
        .unwrap_or(0)
        .min(name_cap)
        .min(width.saturating_sub(fig_w + 4 + MIN_BAR).max(4));
    let bar_w = width.saturating_sub(name_w + fig_w + 4);
    rows.iter()
        .zip(columns)
        .map(|(r, figure)| {
            let note_w = text_width(&r.note);
            let name = cut_middle(&r.name, name_w.saturating_sub(note_w));
            let used = text_width(&name) + note_w;
            let pad = " ".repeat(name_w.saturating_sub(used));
            let mut spans = vec![
                Span::styled(name, r.name_style),
                Span::styled(r.note.clone(), ctx.dim()),
                Span::raw(format!("{pad}  ")),
            ];
            if bar_w >= MIN_BAR {
                let bar_w = u16::try_from(bar_w).unwrap_or(u16::MAX);
                spans.extend(single_bar(r.fraction, bar_w, r.color));
                spans.push(Span::raw("  "));
            }
            spans.extend(figure);
            Line::from(spans)
        })
        .collect()
}

pub(crate) fn contributors(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
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
    let accent = ctx.colors().accent;
    let mut rows: Vec<BarRow> = Vec::new();
    let mut counts: Vec<(String, String, u64)> = folded
        .slices
        .iter()
        .map(|&(i, n)| {
            let author = stats.authors.get(i);
            let name = author.map_or("?", |a| a.name.as_str());
            let note = author
                .filter(|a| a.emails.len() > 1)
                .map_or_else(String::new, |a| format!(" ({} emails)", a.emails.len()));
            (name.to_owned(), note, n)
        })
        .collect();
    if folded.others > 0 {
        counts.push(("others".to_owned(), String::new(), folded.others));
    }
    for ((name, note, n), share) in counts.iter().zip(&split.items) {
        rows.push(BarRow {
            name: name.clone(),
            name_style: Style::new(),
            note: note.clone(),
            fraction: fraction(*n, split.total),
            color: accent,
            figure: figure(*share, split.total, ctx.view.show_counts),
        });
    }
    let mut lines = bar_rows(ctx, area.width, &rows, 26);
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

/// `lines  +84 %  (87.5k) ━━━━━━━━────  −16 %  (16.1k)`: one row, a stacked bar of
/// added over removed with the two shares at its ends. The figures are ink and
/// dim text, only the bar wears the add and delete colours.
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
    // The primary figure and the dim one after it.
    let part = |sign: &str, share: &Share, n: u64| -> (String, String) {
        if split.total < MIN_WHOLE {
            return (format!("{sign}{}", compact(n)), String::new());
        }
        let percent = if share.under_one {
            "<1 %".to_owned()
        } else {
            share
                .percent
                .map_or_else(|| "–".to_owned(), |p| format!("{p} %"))
        };
        if ctx.view.show_counts {
            (format!("{sign}{}", compact(n)), format!("  ({percent})"))
        } else {
            (format!("{sign}{percent}"), format!("  ({})", compact(n)))
        }
    };
    let (Some(added), Some(removed)) = (split.items.first(), split.items.get(1)) else {
        return Vec::new();
    };
    let (add_main, add_rest) = part("+", added, lines.added);
    let (del_main, del_rest) = part("−", removed, lines.removed);
    let fixed = 7
        + text_width(&add_main)
        + text_width(&add_rest)
        + text_width(&del_main)
        + text_width(&del_rest)
        + 4;
    let bar = usize::from(width).saturating_sub(fixed);
    let mut spans = vec![
        Span::styled("lines  ", dim),
        Span::raw(add_main),
        Span::styled(add_rest, dim),
        Span::raw("  "),
    ];
    if bar >= MIN_BAR {
        let parts = [(lines.added, colors.add), (lines.removed, colors.del)];
        spans.extend(stacked_bar(&parts, u16::try_from(bar).unwrap_or(u16::MAX)).spans);
        spans.push(Span::raw("  "));
    }
    spans.push(Span::raw(del_main));
    spans.push(Span::styled(del_rest, dim));
    vec![Line::from(spans)]
}

/// The dim words after the Hot files title.
pub(crate) fn hot_unit(ctx: &Ctx<'_>) -> &'static str {
    match &ctx.stats.hot_files {
        Some(hot) if count(hot.commits) < MIN_WHOLE => "commits touching",
        _ => "share of commits touching",
    }
}

/// The dim footer under the list: `2 files hidden (CHANGELOG.md, Cargo.lock)` and
/// `3 files no longer in the tree`, on one line when they fit, else on two.
pub(crate) fn hot_footer(hot: &HotFiles, width: usize) -> Vec<String> {
    let mut notes = Vec::new();
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
        notes.push(format!(
            "{} hidden ({}{more})",
            plural(hot.hidden.len(), "file", "files"),
            named.join(", ")
        ));
    }
    if hot.gone > 0 {
        notes.push(format!(
            "{} no longer in the tree",
            plural(hot.gone, "file", "files")
        ));
    }
    let joined = notes.join(" · ");
    if text_width(&joined) <= width {
        return if joined.is_empty() {
            Vec::new()
        } else {
            vec![joined]
        };
    }
    notes.iter().map(|n| cut_end(n, width)).collect()
}

pub(crate) fn hot_files(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
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
            note: String::new(),
            fraction: fraction(f.share.count, whole),
            color: ctx.colors().accent,
            figure: figure(f.share, whole, ctx.view.show_counts),
        })
        .collect();
    let mut lines = bar_rows(ctx, area.width, &rows, 28);
    lines.extend(
        hot_footer(hot, usize::from(area.width))
            .into_iter()
            .map(|text| Line::styled(text, ctx.dim())),
    );
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

pub(crate) fn branches(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
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
        let dot = if b.current { " ●" } else { "  " };
        let mut spans = vec![Span::styled(format!("{name}{dot}{pad}  "), style)];
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
pub(crate) fn progress_line(ctx: &Ctx<'_>) -> Line<'static> {
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

/// The stat tiles under the header, two rows: the values in bold ink (the commit
/// count, the hero figure, in the bold accent) over dim labels, separated by
/// spacing only. Tiles that do not fit the width are left out, last first.
pub(crate) fn tiles(ctx: &Ctx<'_>, width: u16) -> [Line<'static>; 2] {
    let t = &ctx.stats.totals;
    let noun = |n: usize, one: &str, many: &str| (if n == 1 { one } else { many }).to_owned();
    let remote = if t.remote_branches > 0 {
        format!(" (+{} remote)", t.remote_branches)
    } else {
        String::new()
    };
    // The commits are those on the main branch: the label says which.
    let on = ctx
        .stats
        .main_branch
        .as_ref()
        .map_or_else(String::new, |main| format!(" on {main}"));
    let mut items = vec![
        (
            t.commits.to_string(),
            format!("{}{on}", noun(t.commits, "commit", "commits")),
        ),
        (t.authors.to_string(), noun(t.authors, "author", "authors")),
        (
            format!("{}{remote}", t.local_branches),
            noun(t.local_branches, "branch", "branches"),
        ),
        (t.tags.to_string(), noun(t.tags, "tag", "tags")),
    ];
    if let Some(since) = &ctx.stats.since_tag {
        items.push((since.commits.to_string(), format!("since {}", since.name)));
    }
    let widths: Vec<usize> = items
        .iter()
        .map(|(v, l)| text_width(v).max(text_width(l)))
        .collect();
    let total = |gap: usize, n: usize| -> usize {
        widths.iter().take(n).sum::<usize>() + gap * n.saturating_sub(1)
    };
    let room = usize::from(width);
    let gap = if total(4, items.len()) <= room { 4 } else { 2 };
    let shown = (1..=items.len())
        .rev()
        .find(|&n| total(gap, n) <= room)
        .unwrap_or(1);
    let hero = Style::new()
        .fg(ctx.colors().accent)
        .add_modifier(Modifier::BOLD);
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let (mut values, mut labels) = (Vec::new(), Vec::new());
    for (i, ((value, label), w)) in items.into_iter().zip(widths).take(shown).enumerate() {
        let (v_pad, l_pad) = (
            " ".repeat(w - text_width(&value) + gap),
            " ".repeat(w - text_width(&label) + gap),
        );
        values.push(Span::styled(value, if i == 0 { hero } else { bold }));
        values.push(Span::raw(v_pad));
        labels.push(Span::styled(label, ctx.dim()));
        labels.push(Span::raw(l_pad));
    }
    [Line::from(values), Line::from(labels)]
}

/// `Commits 423 · Authors 2 · Branches 6 (+3 remote) · Tags 5 · 35 commits since v0.7.0`.
pub(crate) fn totals_line(ctx: &Ctx<'_>) -> Line<'static> {
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
