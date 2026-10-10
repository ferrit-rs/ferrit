//! Dashboard projections and rendering.

use crate::git::stats::{NUMSTAT_CAP, RepoStats, WALK_CAP};
use crate::tui::components::dashboard::charts;
use crate::tui::components::dashboard::tables;
use crate::tui::components::dashboard::text::{date, thousands, window_label};
use crate::tui::draw::{Landed, RenderState, unix_now};
use crate::tui::scene::Scene;
use crate::tui::widgets::chart_palette::{ChartMode, ChartPalette, charts_mode_from_env};
use crate::tui::widgets::chrome::drawer::Drawer;
use crate::tui::widgets::chrome::lists::ScrollBar;
use crate::tui::widgets::chrome::panel::Panel;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

/// Two columns from this width.
pub const WIDE: u16 = 110;
/// One column from this width; under it only the totals and the work in progress.
pub const NARROW: u16 = 60;
/// The page is never wider than this: past it the margins grow, not the charts.
pub const MAX_WIDTH: u16 = 110;
const GUTTER: u16 = 4;
const PAD: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chrome {
    Framed,
    Bare,
}

impl Chrome {
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
    pub stats: Option<&'a RepoStats>,
    pub repo: &'a str,
    pub branch: &'a str,
    pub colors: ChartPalette,
    pub mode: ChartMode,
    pub show_counts: bool,
    pub computing: bool,
    pub churn_pending: bool,
    pub error: Option<&'a str>,
    pub scroll: usize,
    pub chrome: Chrome,
    pub now: i64,
}

pub(crate) struct Ctx<'a> {
    pub stats: &'a RepoStats,
    pub view: &'a View<'a>,
}

impl Ctx<'_> {
    pub(crate) fn colors(&self) -> &ChartPalette {
        &self.view.colors
    }

    pub(crate) fn dim(&self) -> Style {
        self.view.colors.dim
    }
}

pub(crate) fn note(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
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

    fn rows(self, ctx: &Ctx<'_>, width: u16) -> (u16, u16) {
        let stats = ctx.stats;
        let rows = |count: usize| u16::try_from(count).unwrap_or(u16::MAX);
        match self {
            Self::Activity => (5, 8),
            Self::Kinds => (6, 8),
            Self::Heat => (5, 10),
            Self::Contributors => {
                let want = rows(stats.authors.len().min(6) + 1);
                (want.min(4), want)
            },
            Self::Hot => {
                let files = stats.hot_files.as_ref().map_or(1, |hot| {
                    hot.files.len().max(1) + tables::hot_footer(hot, usize::from(width)).len()
                });
                let want = rows(files);
                (want.min(4), want)
            },
            Self::Branches => {
                let count = stats.branches.len();
                let want = rows(
                    1 + count.min(tables::BRANCH_ROWS) + usize::from(count > tables::BRANCH_ROWS),
                );
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

struct Band {
    sections: Vec<Section>,
    rows: u16,
}

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
        .map(|section| vec![section])
        .collect()
    };
    let sizes: Vec<(u16, u16)> = groups
        .iter()
        .map(|group| {
            group.iter().fold((0, 0), |(minimum, maximum), section| {
                let (section_minimum, section_maximum) = section.rows(ctx, width);
                (minimum.max(section_minimum), maximum.max(section_maximum))
            })
        })
        .collect();
    let mut rows: Vec<u16> = sizes.iter().map(|&(_, want)| want).collect();
    if wide {
        let minimums: u16 = sizes.iter().map(|&(minimum, _)| minimum).sum();
        let wants: u16 = rows.iter().sum();
        if budget < wants {
            let mut over = wants - budget.max(minimums);
            for (row, &(minimum, _)) in rows.iter_mut().zip(&sizes).rev() {
                let take = over.min(row.saturating_sub(minimum));
                *row -= take;
                over -= take;
            }
        } else if let Some(first) = rows.first_mut() {
            *first += (budget - wants).min(8);
        }
    }
    groups
        .into_iter()
        .zip(rows)
        .map(|(sections, rows)| Band { sections, rows })
        .collect()
}

fn notices(ctx: &Ctx<'_>) -> Vec<Line<'static>> {
    let warn = Style::new().fg(ctx.colors().warn);
    let stats = ctx.stats;
    let mut output = Vec::new();
    if stats.shallow {
        let since = stats
            .totals
            .first_commit
            .map_or_else(|| "the first commit".to_owned(), date);
        output.push(Line::styled(
            format!("shallow: history before {since} is not available"),
            warn,
        ));
    }
    if stats.sampled {
        output.push(Line::styled(
            format!(
                "sampled: newest {} commits read, lines from the newest {}",
                thousands(WALK_CAP),
                thousands(NUMSTAT_CAP)
            ),
            warn,
        ));
    }
    if let Some(error) = ctx.view.error {
        output.push(Line::styled(format!("stats error: {error}"), warn));
    }
    output
}

fn frame_block(view: &View<'_>) -> Block<'static> {
    Panel::new().border_style(view.colors.rule).block()
}

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
    let mut output = vec![blank.clone()];
    output.extend(tables::tiles(ctx, inner_w));
    output.push(blank.clone());
    let notices = notices(ctx);
    if !notices.is_empty() {
        output.extend(notices);
        output.push(blank.clone());
    }
    if ctx.stats.totals.first_commit.is_none() && ctx.stats.totals.commits == 0 {
        output.push(Line::styled("no commits yet", ctx.dim()));
        output.push(blank);
    }
    output
}

fn compose(view: &View<'_>, width: u16, height: u16) -> Buffer {
    if width < NARROW {
        return compact(view, width, height);
    }
    let ctx = view.stats.map(|stats| Ctx { stats, view });
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
    let charts = ctx.as_ref().is_some_and(|context| {
        context.stats.totals.first_commit.is_some() || context.stats.totals.commits > 0
    });
    let progress_rows = u16::from(ctx.is_some());
    let bands = match &ctx {
        Some(context) if charts => {
            let count = if wide { 3 } else { 6 };
            let chrome = 1 + 2 * rim + body_rows + 3 * count + progress_rows;
            bands(context, wide, col_w, height.saturating_sub(chrome))
        },
        _ => Vec::new(),
    };
    let band_rows: u16 = bands.iter().map(|band| band.rows + 3).sum();
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
        for (section, (x, width)) in band.sections.iter().zip(columns) {
            let (title, unit) = section.title(ctx, width);
            let mut spans = vec![Span::styled(title, bold)];
            if !unit.is_empty() {
                spans.push(Span::styled(format!("  {unit}"), view.colors.dim));
            }
            Paragraph::new(Line::from(spans)).render(Rect::new(x, y, width, 1), &mut buf);
            Paragraph::new(Line::styled("─".repeat(usize::from(width)), rule))
                .render(Rect::new(x, y + 1, width, 1), &mut buf);
            section.draw(ctx, Rect::new(x, y + 2, width, band.rows), &mut buf);
        }
        y += band.rows + 3;
    }
    Paragraph::new(tables::progress_line(ctx)).render(Rect::new(PAD, y, inner_w, 1), &mut buf);
    buf
}

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

const MAX_PERCENT: u16 = 95;

pub(crate) fn draw_sheet(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &Scene<'_>,
    render: &mut RenderState,
    landed: &mut Landed,
) {
    let accent = app.theme.config.color();
    let width = (MAX_WIDTH + 2).min(area.width.saturating_mul(MAX_PERCENT) / 100);
    let Some(inner) = Drawer::new(&mut render.sheet, " Dashboard ")
        .width(Constraint::Length(width))
        .border_style(Style::new().fg(accent))
        .render(frame, area)
    else {
        return;
    };
    let view = View {
        stats: app.dashboard().stats(),
        repo: app.repo_name,
        branch: &app.snapshot.header.branch,
        colors: ChartPalette {
            density: std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()),
            ..ChartPalette::for_palette(&app.palette())
        },
        mode: charts_mode_from_env(),
        show_counts: app.dashboard().show_counts(),
        computing: app.dashboard().computing(),
        churn_pending: app.dashboard().churn_pending(),
        error: app.dashboard().error(),
        scroll: app.dashboard().scroll(),
        chrome: Chrome::Bare,
        now: unix_now(),
    };
    let max_scroll = draw(frame, inner, &view);
    landed.dashboard_max_scroll = Some(max_scroll);
}
