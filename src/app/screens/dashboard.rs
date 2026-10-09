//! The dashboard screen (`docs/PLAN_13_DASHBOARD.md`, "Rendering"): a pure
//! function of the statistics, the area and the colours. It draws the page on
//! an off-screen buffer as tall as it needs to be, then copies the rows that
//! `scroll` selects, so every layout scrolls the same way and the largest useful
//! offset comes back to the caller (the app clamps its scroll to it).
//!
//! Layouts: 110 columns and up, two columns as in the plan's diagram; 60 to 109,
//! one column of stacked sections; under 60, the totals and the work in
//! progress only. The page is one rounded border in the recessive rule colour,
//! at most `MAX_WIDTH` wide and centred; inside it there are no boxes, only a
//! bold title over a thin dim rule for each section.

mod charts;
mod tables;
mod text;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use self::text::{date, thousands, window_label};
use crate::components::ui::chart_palette::{ChartMode, ChartPalette};
use crate::components::ui::panel::Panel;
use crate::components::ui::scroll_bar::ScrollBar;
use crate::git::stats::{NUMSTAT_CAP, RepoStats, WALK_CAP};

/// Two columns from this width.
pub const WIDE: u16 = 110;
/// One column from this width; under it only the totals and the work in progress.
pub const NARROW: u16 = 60;
/// The page is never wider than this: past it the margins grow, not the charts.
pub const MAX_WIDTH: u16 = 110;
/// Columns between the two columns of the wide layout.
const GUTTER: u16 = 4;
/// Columns between the border and the content.
const PAD: u16 = 2;

/// Whether the page draws its own rounded border and its "Dashboard" title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chrome {
    /// A page of its own: the border and the title.
    Framed,
    /// Inside a frame that already has them (the dashboard sheet).
    Bare,
}

impl Chrome {
    /// The rows a border takes at the top, and again at the bottom.
    const fn rim(self) -> u16 {
        match self {
            Self::Framed => 1,
            Self::Bare => 0,
        }
    }
}

/// What the screen draws, all of it given: nothing here reads the clock or the app.
#[derive(Debug)]
pub struct View<'a> {
    /// `None` until the first pass arrives, or when it failed.
    pub stats: Option<&'a RepoStats>,
    pub repo: &'a str,
    /// The checked-out branch.
    pub branch: &'a str,
    pub colors: ChartPalette,
    pub mode: ChartMode,
    /// `n`: counts first, percentages in brackets.
    pub show_counts: bool,
    /// Nothing has arrived yet.
    pub computing: bool,
    /// The lines and hot files are still being read.
    pub churn_pending: bool,
    pub error: Option<&'a str>,
    pub scroll: usize,
    /// Draw the page's own rounded border and its "Dashboard" title. A sheet has
    /// its own frame and title, so it asks for none (`screens/dashboard_sheet.rs`).
    pub chrome: Chrome,
    /// Unix seconds: the end of the series and the base of relative times.
    pub now: i64,
}

/// A view with stats in hand.
pub(super) struct Ctx<'a> {
    pub stats: &'a RepoStats,
    pub view: &'a View<'a>,
}

impl Ctx<'_> {
    pub(super) fn colors(&self) -> &ChartPalette {
        &self.view.colors
    }

    pub(super) fn dim(&self) -> Style {
        self.view.colors.dim
    }
}

/// One line of text at the top of `area`.
pub(super) fn note(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
    Paragraph::new(Line::styled(text.to_owned(), style)).render(area, buf);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Activity,
    Kinds,
    Heat,
    Contributors,
    Hot,
    Branches,
}

impl Section {
    /// The bold title and the dim words after it.
    fn title(self, ctx: &Ctx<'_>, width: u16) -> (&'static str, String) {
        match self {
            Self::Activity => ("Activity", charts::activity_unit(ctx)),
            Self::Kinds => ("What was done", String::new()),
            Self::Heat => ("Commits per day", charts::heat_unit(width)),
            Self::Contributors => ("Contributors", String::new()),
            Self::Hot => ("Hot files", tables::hot_unit(ctx).to_owned()),
            Self::Branches => ("Branches", String::new()),
        }
    }

    /// Content rows in a column `width` wide: the least that still reads, and
    /// what fills the section.
    fn rows(self, ctx: &Ctx<'_>, width: u16) -> (u16, u16) {
        let stats = ctx.stats;
        let rows = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
        match self {
            Self::Activity => (5, 8),
            Self::Kinds => (6, 8),
            Self::Heat => (5, 10),
            Self::Contributors => {
                let want = rows(stats.authors.len().min(6) + 1);
                (want.min(4), want)
            },
            Self::Hot => {
                let files = stats.hot_files.as_ref().map_or(1, |h| {
                    h.files.len().max(1) + tables::hot_footer(h, usize::from(width)).len()
                });
                let want = rows(files);
                (want.min(4), want)
            },
            Self::Branches => {
                let n = stats.branches.len();
                let want =
                    rows(1 + n.min(tables::BRANCH_ROWS) + usize::from(n > tables::BRANCH_ROWS));
                (want.min(5), want)
            },
        }
    }

    fn draw(self, ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
        match self {
            Self::Activity => charts::activity(ctx, area, buf),
            Self::Kinds => charts::kinds(ctx, area, buf),
            Self::Heat => charts::heat(ctx, area, buf),
            Self::Contributors => tables::contributors(ctx, area, buf),
            Self::Hot => tables::hot_files(ctx, area, buf),
            Self::Branches => tables::branches(ctx, area, buf),
        }
    }
}

/// One horizontal band of the page: one or two sections side by side.
struct Band {
    sections: Vec<Section>,
    rows: u16,
}

/// The bands for `wide` or stacked, each at the rows it wants; when `budget` is
/// given (wide) and short, the rows shrink towards the minimum.
fn bands(ctx: &Ctx<'_>, wide: bool, width: u16, budget: u16) -> Vec<Band> {
    let groups: Vec<Vec<Section>> = if wide {
        vec![
            vec![Section::Activity, Section::Kinds],
            vec![Section::Heat, Section::Contributors],
            vec![Section::Hot, Section::Branches],
        ]
    } else {
        [
            Section::Activity,
            Section::Kinds,
            Section::Heat,
            Section::Contributors,
            Section::Hot,
            Section::Branches,
        ]
        .into_iter()
        .map(|s| vec![s])
        .collect()
    };
    let sizes: Vec<(u16, u16)> = groups
        .iter()
        .map(|g| {
            g.iter().fold((0, 0), |(lo, hi), s| {
                let (a, b) = s.rows(ctx, width);
                (lo.max(a), hi.max(b))
            })
        })
        .collect();
    let mut rows: Vec<u16> = sizes.iter().map(|&(_, want)| want).collect();
    if wide {
        let mins: u16 = sizes.iter().map(|&(min, _)| min).sum();
        let wants: u16 = rows.iter().sum();
        if budget < wants {
            // Take rows back from the last band first, never under its minimum.
            let mut over = wants - budget.max(mins);
            for (row, &(min, _)) in rows.iter_mut().zip(&sizes).rev() {
                let take = over.min(row.saturating_sub(min));
                *row -= take;
                over -= take;
            }
        } else if let Some(first) = rows.first_mut() {
            // Spare room grows the line chart, within reason.
            *first += (budget - wants).min(8);
        }
    }
    groups
        .into_iter()
        .zip(rows)
        .map(|(sections, rows)| Band { sections, rows })
        .collect()
}

/// Lines under the totals: shallow clone, sampling, an error next to stats.
fn notices(ctx: &Ctx<'_>) -> Vec<Line<'static>> {
    let warn = Style::new().fg(ctx.colors().warn);
    let stats = ctx.stats;
    let mut out = Vec::new();
    if stats.shallow {
        let since = stats
            .totals
            .first_commit
            .map_or_else(|| "the first commit".to_owned(), date);
        out.push(Line::styled(
            format!("shallow: history before {since} is not available"),
            warn,
        ));
    }
    if stats.sampled {
        out.push(Line::styled(
            format!(
                "sampled: newest {} commits read, lines from the newest {}",
                thousands(WALK_CAP),
                thousands(NUMSTAT_CAP)
            ),
            warn,
        ));
    }
    if let Some(error) = ctx.view.error {
        out.push(Line::styled(format!("stats error: {error}"), warn));
    }
    out
}

/// The border of the whole page: one rounded line in the recessive rule colour.
fn frame_block(view: &View<'_>) -> Block<'static> {
    Panel::new().border_style(view.colors.rule).block()
}

/// `ferrit · main   Dashboard` on the left, `window: 90 days (t)` on the right.
fn header(view: &View<'_>, area: Rect, buf: &mut Buffer) {
    let dim = view.colors.dim;
    Paragraph::new(Line::from(vec![
        Span::styled(
            view.repo.to_owned(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", dim),
        Span::raw(view.branch.to_owned()),
        Span::styled(
            if view.chrome == Chrome::Framed {
                "   Dashboard"
            } else {
                ""
            },
            dim,
        ),
    ]))
    .render(area, buf);
    if let Some(stats) = view.stats {
        Line::styled(format!("window: {} (t)", window_label(stats.window)), dim)
            .right_aligned()
            .render(area, buf);
    }
}

/// Under `NARROW` columns: the totals and the work in progress, and a hint.
fn compact(view: &View<'_>, width: u16, height: u16) -> Buffer {
    let lines: Vec<Line<'static>> = match view.stats.map(|stats| Ctx { stats, view }) {
        Some(ctx) => vec![
            tables::totals_line(&ctx),
            tables::progress_line(&ctx),
            Line::styled("widen the terminal for charts", ctx.dim()),
        ],
        None => vec![Line::raw(match view.error {
            Some(error) => format!("could not read the statistics: {error}"),
            None => "computing…".to_owned(),
        })],
    };
    // The rows the border takes, top and bottom; the header is one more.
    let rim = view.chrome.rim();
    let chrome = 1 + 2 * rim;
    let page = chrome + u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let area = Rect::new(0, 0, width, page.max(u16::from(height > 0)));
    let mut buf = Buffer::empty(area);
    if view.chrome == Chrome::Framed {
        frame_block(view).render(area, &mut buf);
    }
    let inner_w = width.saturating_sub(2 * PAD);
    header(view, Rect::new(PAD, rim, inner_w, 1), &mut buf);
    Paragraph::new(lines).render(
        Rect::new(PAD, rim + 1, inner_w, page.saturating_sub(chrome)),
        &mut buf,
    );
    buf
}

/// The lines between the header and the first section: a blank, the stat
/// tiles, the notices, and the "no commits yet" of an empty repository.
fn body(view: &View<'_>, ctx: Option<&Ctx<'_>>, inner_w: u16) -> Vec<Line<'static>> {
    let blank = Line::raw("");
    let Some(ctx) = ctx else {
        return vec![
            blank,
            Line::raw(match view.error {
                Some(error) => format!("could not read the statistics: {error}"),
                None => "computing…".to_owned(),
            }),
        ];
    };
    let mut out = vec![blank.clone()];
    out.extend(tables::tiles(ctx, inner_w));
    out.push(blank.clone());
    let notices = notices(ctx);
    if !notices.is_empty() {
        out.extend(notices);
        out.push(blank.clone());
    }
    if ctx.stats.totals.first_commit.is_none() && ctx.stats.totals.commits == 0 {
        out.push(Line::styled("no commits yet", ctx.dim()));
        out.push(blank);
    }
    out
}

/// The whole page on its own buffer, `width` columns wide (already capped) and
/// as tall as its content (or `height`, when the content is shorter).
fn compose(view: &View<'_>, width: u16, height: u16) -> Buffer {
    if width < NARROW {
        return compact(view, width, height);
    }
    let ctx = view.stats.map(|stats| Ctx { stats, view });
    // The rows the border takes, top and bottom (none when the page is not framed).
    let rim = view.chrome.rim();
    let inner_w = width.saturating_sub(2 * PAD);
    let wide = width >= WIDE;
    let col_w = if wide {
        inner_w.saturating_sub(GUTTER) / 2
    } else {
        inner_w
    };
    let right_x = PAD + col_w + GUTTER;
    let right_w = inner_w.saturating_sub(col_w + GUTTER);
    let body = body(view, ctx.as_ref(), inner_w);
    let body_rows = u16::try_from(body.len()).unwrap_or(u16::MAX);
    let charts = ctx
        .as_ref()
        .is_some_and(|c| c.stats.totals.first_commit.is_some() || c.stats.totals.commits > 0);
    // The work in progress is one row under the last section.
    let progress_rows = u16::from(ctx.is_some());
    let bands = match &ctx {
        Some(ctx) if charts => {
            // Borders, header, body, a title, a rule and a blank per band, and
            // the progress line come off the height; the bands share the rest.
            let n = if wide { 3 } else { 6 };
            let chrome = 1 + 2 * rim + body_rows + 3 * n + progress_rows;
            bands(ctx, wide, col_w, height.saturating_sub(chrome))
        },
        _ => Vec::new(),
    };
    let band_rows: u16 = bands.iter().map(|b| b.rows + 3).sum();
    let page = 1 + 2 * rim + body_rows + band_rows + progress_rows;
    let area = Rect::new(0, 0, width, page.max(u16::from(height > 0)));
    let mut buf = Buffer::empty(area);
    if view.chrome == Chrome::Framed {
        frame_block(view).render(area, &mut buf);
    }
    header(view, Rect::new(PAD, rim, inner_w, 1), &mut buf);
    let mut y = rim + 1;
    for line in body {
        Paragraph::new(line).render(Rect::new(PAD, y, inner_w, 1), &mut buf);
        y += 1;
    }
    let Some(ctx) = &ctx else {
        return buf;
    };
    let rule = view.colors.rule;
    let bold = Style::new().add_modifier(Modifier::BOLD);
    for band in &bands {
        let columns = if band.sections.len() == 2 {
            vec![(PAD, col_w), (right_x, right_w)]
        } else {
            vec![(PAD, inner_w)]
        };
        for (section, (x, w)) in band.sections.iter().zip(columns) {
            let (title, unit) = section.title(ctx, w);
            let mut spans = vec![Span::styled(title, bold)];
            if !unit.is_empty() {
                spans.push(Span::styled(format!("  {unit}"), view.colors.dim));
            }
            Paragraph::new(Line::from(spans)).render(Rect::new(x, y, w, 1), &mut buf);
            Paragraph::new(Line::styled("─".repeat(usize::from(w)), rule))
                .render(Rect::new(x, y + 1, w, 1), &mut buf);
            section.draw(ctx, Rect::new(x, y + 2, w, band.rows), &mut buf);
        }
        y += band.rows + 3;
    }
    Paragraph::new(tables::progress_line(ctx)).render(Rect::new(PAD, y, inner_w, 1), &mut buf);
    buf
}

/// Draw the dashboard into `area` of `frame`, scrolled by `view.scroll`
/// (clamped). Returns the largest offset that shows anything new, for the app
/// to clamp its own scroll to.
pub fn draw(frame: &mut Frame<'_>, area: Rect, view: &View<'_>) -> usize {
    frame.render_widget(Clear, area);
    let page_w = area.width.min(MAX_WIDTH);
    let margin = (area.width - page_w) / 2;
    let page = compose(view, page_w, area.height);
    let page_rows = page.area.height;
    let max_scroll = usize::from(page_rows.saturating_sub(area.height));
    let scroll = view.scroll.min(max_scroll);
    let first = u16::try_from(scroll).unwrap_or(0);
    let out = frame.buffer_mut();
    for dy in 0..area.height.min(page_rows) {
        for x in 0..page_w {
            if let (Some(from), Some(to)) = (
                page.cell((x, dy + first)),
                out.cell_mut((area.x + margin + x, area.y + dy)),
            ) {
                to.clone_from(from);
            }
        }
    }
    let track = Rect::new(area.right().saturating_sub(1), area.y, 1, area.height);
    ScrollBar::new(usize::from(page_rows), usize::from(area.height), scroll)
        .style(Style::new().fg(view.colors.accent))
        .render(frame, track);
    max_scroll
}
