//! The dashboard screen (`docs/PLAN_13_DASHBOARD.md`, "Rendering"): a pure
//! function of the statistics, the area and the colours. It draws the page on
//! an off-screen buffer as tall as it needs to be, then copies the rows that
//! `scroll` selects, so every layout scrolls the same way and the largest useful
//! offset comes back to the caller (the app clamps its scroll to it).
//!
//! Layouts: 110 columns and up, two columns as in the plan's diagram; 60 to 109,
//! one column of stacked sections; under 60, the totals and the work in
//! progress only.

mod charts;
mod tables;
mod text;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Clear, Paragraph, Widget};

use self::text::{date, thousands, window_label};
use crate::components::ui::chart_palette::{ChartMode, ChartPalette};
use crate::components::ui::panel::Panel;
use crate::components::ui::scroll_bar::ScrollBar;
use crate::domain::git::stats::{NUMSTAT_CAP, RepoStats, WALK_CAP};

/// Two columns from this width.
pub const WIDE: u16 = 110;
/// One column from this width; under it only the totals and the work in progress.
pub const NARROW: u16 = 60;

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
    fn title(self, ctx: &Ctx<'_>, width: u16) -> String {
        match self {
            Self::Activity => format!("Activity ({})", charts::activity_unit(ctx)),
            Self::Kinds => "What was done".to_owned(),
            Self::Heat => charts::heat_title(width),
            Self::Contributors => "Contributors".to_owned(),
            Self::Hot => tables::hot_title(ctx).to_owned(),
            Self::Branches => "Branches".to_owned(),
        }
    }

    /// Content rows: the least that still reads, and what fills the section.
    fn rows(self, ctx: &Ctx<'_>) -> (u16, u16) {
        let stats = ctx.stats;
        let rows = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
        match self {
            Self::Activity | Self::Kinds => (6, 8),
            Self::Heat => (5, 10),
            Self::Contributors => {
                let want = rows(stats.authors.len().min(6) + 2);
                (want.min(4), want)
            },
            Self::Hot => {
                let files = stats.hot_files.as_ref().map_or(1, |h| {
                    h.files.len().max(1) + usize::from(!h.hidden.is_empty())
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
fn bands(ctx: &Ctx<'_>, wide: bool, budget: u16) -> Vec<Band> {
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
                let (a, b) = s.rows(ctx);
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

/// The border of the whole page, with the title and the window.
fn frame_block(view: &View<'_>) -> ratatui::widgets::Block<'static> {
    let accent = Style::new().fg(view.colors.accent);
    let title = format!(" Dashboard ─ {} ─ {} ", view.repo, view.branch);
    let window = view.stats.map_or(String::new(), |s| {
        format!(" window: {} (t) ", window_label(s.window))
    });
    Panel::new()
        .title(Line::styled(title, accent.add_modifier(Modifier::BOLD)))
        .border_style(accent)
        .block()
        .title(Line::styled(window, Style::new()).right_aligned())
}

/// Where the column bar sits in a page `width` wide: the left column takes the
/// larger half.
fn split_column(width: u16) -> u16 {
    1 + width.saturating_sub(3).div_ceil(2)
}

/// A horizontal rule across the page, `├ Title ───┬ Title ───┤`, joined to the
/// column bar above (`above_two`) and below (`below_two`) it.
fn paint_divider(buf: &mut Buffer, y: u16, width: u16, style: Style, rule: &Rule<'_>) {
    let right = width.saturating_sub(1);
    for x in 1..right {
        buf.set_string(x, y, "─", style);
    }
    buf.set_string(0, y, "├", style);
    buf.set_string(right, y, "┤", style);
    let bold = style.add_modifier(Modifier::BOLD);
    let limit = if rule.below_two { rule.split } else { right };
    buf.set_stringn(
        1,
        y,
        format!(" {} ", rule.left),
        usize::from(limit.saturating_sub(1)),
        bold,
    );
    if let Some(title) = rule.right.filter(|_| rule.below_two) {
        buf.set_stringn(
            rule.split + 1,
            y,
            format!(" {title} "),
            usize::from(right.saturating_sub(rule.split + 1)),
            bold,
        );
    }
    let joint = match (rule.above_two, rule.below_two) {
        (false, true) => "┬",
        (true, true) => "┼",
        (true, false) => "┴",
        (false, false) => return,
    };
    buf.set_string(rule.split, y, joint, style);
}

/// The titles and joints of one rule.
struct Rule<'a> {
    left: &'a str,
    right: Option<&'a str>,
    split: u16,
    above_two: bool,
    below_two: bool,
}

/// Under `NARROW` columns: the totals and the work in progress, and a hint.
fn compact(view: &View<'_>, width: u16, height: u16) -> Buffer {
    let lines: Vec<Line<'static>> = match view.stats.map(|stats| Ctx { stats, view }) {
        Some(ctx) => vec![
            tables::totals_line(&ctx),
            tables::progress_line(&ctx),
            Line::styled("widen the terminal for charts", ctx.dim()),
        ],
        None => vec![Line::styled(
            match view.error {
                Some(error) => format!("could not read the statistics: {error}"),
                None => "computing…".to_owned(),
            },
            Style::new(),
        )],
    };
    let page = 2 + u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let area = Rect::new(0, 0, width, page.max(u16::from(height > 0)));
    let mut buf = Buffer::empty(area);
    frame_block(view).render(area, &mut buf);
    Paragraph::new(lines).render(
        Rect::new(1, 1, width.saturating_sub(2), page.saturating_sub(2)),
        &mut buf,
    );
    buf
}

/// The whole page on its own buffer, `width` columns wide and as tall as its
/// content (or `height`, when the content is shorter and wide).
fn compose(view: &View<'_>, width: u16, height: u16) -> Buffer {
    if width < NARROW {
        return compact(view, width, height);
    }
    let ctx = view.stats.map(|stats| Ctx { stats, view });
    let mut top: Vec<Line<'static>> = Vec::new();
    let mut charts = false;
    match &ctx {
        None => top.push(Line::raw(match view.error {
            Some(error) => format!("could not read the statistics: {error}"),
            None => "computing…".to_owned(),
        })),
        Some(ctx) => {
            top.push(tables::totals_line(ctx));
            top.extend(notices(ctx));
            if ctx.stats.totals.first_commit.is_none() && ctx.stats.totals.commits == 0 {
                top.push(Line::styled("no commits yet", ctx.dim()));
            } else {
                charts = true;
            }
        },
    }
    let wide = width >= WIDE;
    let top_rows = u16::try_from(top.len()).unwrap_or(u16::MAX);
    // In progress is a rule and a row.
    let progress_rows = 2 * u16::from(ctx.is_some());
    let bands = match &ctx {
        Some(ctx) if charts => {
            // Borders, totals and notices, one rule per band and In progress
            // come off the height; the bands share what is left.
            let rules: u16 = if wide { 3 } else { 6 };
            let chrome = 2 + top_rows + rules + progress_rows;
            bands(ctx, wide, height.saturating_sub(chrome))
        },
        _ => Vec::new(),
    };
    let band_rows: u16 = bands.iter().map(|b| b.rows + 1).sum();
    let page = 2 + top_rows + band_rows + progress_rows;
    let area = Rect::new(0, 0, width, page.max(u16::from(height > 0)));
    let mut buf = Buffer::empty(area);
    frame_block(view).render(area, &mut buf);
    let inner_w = width.saturating_sub(2);
    let mut y = 1;
    for line in top {
        Paragraph::new(line).render(Rect::new(1, y, inner_w, 1), &mut buf);
        y += 1;
    }
    let Some(ctx) = &ctx else {
        return buf;
    };
    let style = Style::new().fg(view.colors.accent);
    let split = split_column(width);
    let mut above_two = false;
    for band in &bands {
        let two = band.sections.len() == 2;
        let widths = [split.saturating_sub(3), width.saturating_sub(split + 3)];
        let titles: Vec<String> = band
            .sections
            .iter()
            .zip(if two { widths } else { [inner_w, inner_w] })
            .map(|(section, w)| section.title(ctx, w))
            .collect();
        let rule = Rule {
            left: titles.first().map_or("", String::as_str),
            right: titles.get(1).map(String::as_str),
            split,
            above_two,
            below_two: two,
        };
        paint_divider(&mut buf, y, width, style, &rule);
        y += 1;
        for (i, section) in band.sections.iter().enumerate() {
            let area = match (two, i) {
                (false, _) => Rect::new(1, y, inner_w, band.rows),
                (true, 0) => Rect::new(1, y, split - 1, band.rows),
                (true, _) => Rect::new(split + 1, y, width.saturating_sub(split + 2), band.rows),
            };
            section.draw(ctx, area, &mut buf);
        }
        if two {
            for row in y..y + band.rows {
                buf.set_string(split, row, "│", style);
            }
        }
        y += band.rows;
        above_two = two;
    }
    let rule = Rule {
        left: "In progress",
        right: None,
        split,
        above_two,
        below_two: false,
    };
    paint_divider(&mut buf, y, width, style, &rule);
    Paragraph::new(tables::progress_line(ctx)).render(Rect::new(1, y + 1, inner_w, 1), &mut buf);
    buf
}

/// Draw the dashboard into `area` of `frame`, scrolled by `view.scroll`
/// (clamped). Returns the largest offset that shows anything new, for the app
/// to clamp its own scroll to.
pub fn draw(frame: &mut Frame<'_>, area: Rect, view: &View<'_>) -> usize {
    frame.render_widget(Clear, area);
    let page = compose(view, area.width, area.height);
    let page_rows = page.area.height;
    let max_scroll = usize::from(page_rows.saturating_sub(area.height));
    let scroll = view.scroll.min(max_scroll);
    let first = u16::try_from(scroll).unwrap_or(0);
    let out = frame.buffer_mut();
    for dy in 0..area.height.min(page_rows) {
        for x in 0..area.width {
            if let (Some(from), Some(to)) = (
                page.cell((x, dy + first)),
                out.cell_mut((area.x + x, area.y + dy)),
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
