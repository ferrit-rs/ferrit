//! The three chart sections of the dashboard: Activity (line chart), What was
//! done (donut) and Commits per day (heat map). Each draws into the area it is
//! given and degrades when it is small.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph, Sparkline, Widget};

use super::text::{axis_date, figure, figure_columns, plural};
use super::{Ctx, note};
use crate::components::ui::chart_palette::{ChartMode, OTHERS, kind_slot, slot_marker};
use crate::components::ui::donut::{self, Donut, Slice};
use crate::components::ui::heatmap::{self, HeatMap};
use crate::components::ui::share_bar::stacked_bar;
use crate::domain::git::stats::series::Granularity;
use crate::domain::git::stats::share::{fold, shares};

const DAY: i64 = 86_400;
/// Columns and rows the ring is drawn in: 32 x 32 dots, so it looks round.
const RING_WIDTH: u16 = 16;
const RING_HEIGHT: u16 = 8;
/// Room the legend needs beside the ring.
const LEGEND_MIN: u16 = 22;
/// The heat map with all seven weekdays: header row and seven days.
const HEAT_FULL: u16 = 9;
/// The heat map with three weekdays: header row and three days.
const HEAT_SHORT: u16 = 4;

const SLOT_NAMES: [&str; 6] = ["feat", "fix", "docs", "test", "refactor", "others"];

pub(super) const NO_COMMITS: &str = "no commits in this window";

fn count_f64(n: usize) -> f64 {
    u32::try_from(n).map_or_else(|_| f64::from(u32::MAX), f64::from)
}

/// `commits per day` and `13 weeks of history`, for the title.
pub(super) fn activity_title(ctx: &Ctx<'_>) -> String {
    let (per, unit) = match ctx.stats.granularity {
        Granularity::Day => ("day", "days"),
        Granularity::Week => ("week", "weeks"),
        Granularity::Month => ("month", "months"),
    };
    let n = ctx.stats.series.len();
    if n < 2 {
        format!("Activity (commits per {per})")
    } else {
        format!("Activity (commits per {per}, {n} {unit} of history)")
    }
}

pub(super) fn activity(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let stats = ctx.stats;
    if stats.totals.commits == 0 {
        note(buf, area, NO_COMMITS, ctx.dim());
        return;
    }
    let (Some(first), Some(last)) = (stats.series.first(), stats.series.last()) else {
        return;
    };
    if stats.series.len() < 2 {
        let when = match stats.granularity {
            Granularity::Day => "today",
            Granularity::Week => "this week",
            Granularity::Month => "this month",
        };
        let text = format!(
            "{} {when}",
            plural(stats.totals.commits, "commit", "commits")
        );
        note(buf, area, &text, Style::new());
        return;
    }
    let peak = stats
        .series
        .iter()
        .map(|b| b.commits)
        .max()
        .unwrap_or(1)
        .max(1);
    let long = last.start - first.start > 300 * DAY;
    let from = axis_date(first.start, stats.granularity, long);
    let to = axis_date(last.start, stats.granularity, long);
    let accent = Style::new().fg(ctx.colors().accent);
    match ctx.view.mode {
        ChartMode::Braille => {
            let data: Vec<(f64, f64)> = stats
                .series
                .iter()
                .enumerate()
                .map(|(i, b)| (count_f64(i), count_f64(b.commits)))
                .collect();
            let dataset = Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(accent)
                .data(&data);
            let axis = ctx.dim();
            Chart::new(vec![dataset])
                .x_axis(
                    Axis::default()
                        .style(axis)
                        .bounds([0.0, count_f64(data.len() - 1)])
                        .labels(vec![Span::raw(from), Span::raw(to)]),
                )
                .y_axis(
                    Axis::default()
                        .style(axis)
                        .bounds([0.0, count_f64(peak)])
                        .labels(vec![Span::raw("0"), Span::raw(peak.to_string())]),
                )
                .render(area, buf);
        },
        ChartMode::Blocks => blocks_line(ctx, area, buf, (&from, &to), peak),
    }
}

/// The block-glyph fallback of the line chart: a `Sparkline` with the dates and
/// the peak on the last row.
fn blocks_line(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer, dates: (&str, &str), peak: usize) {
    if area.height < 2 {
        return;
    }
    let width = usize::from(area.width).max(1);
    let counts: Vec<u64> = ctx
        .stats
        .series
        .iter()
        .map(|b| u64::try_from(b.commits).unwrap_or(0))
        .collect();
    // More points than columns: keep the highest of each chunk.
    let step = counts.len().div_ceil(width).max(1);
    let data: Vec<u64> = counts
        .chunks(step)
        .map(|c| c.iter().copied().max().unwrap_or(0))
        .collect();
    let spark = Rect {
        height: area.height - 1,
        ..area
    };
    let labels = Rect::new(area.x, area.bottom() - 1, area.width, 1);
    Sparkline::default()
        .data(&data)
        .max(u64::try_from(peak).unwrap_or(1))
        .style(Style::new().fg(ctx.colors().accent))
        .render(spark, buf);
    let text = format!("{}  →  {}   peak {peak}", dates.0, dates.1);
    Paragraph::new(Line::styled(text, ctx.dim())).render(labels, buf);
}

/// One legend row: marker, name, right-aligned figure.
struct Piece {
    slot: usize,
    name: &'static str,
    count: u64,
}

/// Kinds grouped in the five named slots and gray "others", folded for the donut.
fn pieces(ctx: &Ctx<'_>) -> (Vec<Piece>, u64) {
    let mut per_slot = [0_u64; 6];
    for stat in &ctx.stats.kinds {
        if let Some(n) = per_slot.get_mut(kind_slot(stat.kind)) {
            *n += u64::try_from(stat.commits).unwrap_or(0);
        }
    }
    let total: u64 = per_slot.iter().sum();
    let items: Vec<(usize, u64)> = per_slot
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, n)| n > 0)
        .collect();
    let folded = fold(items, 3, 6);
    let mut others = folded.others;
    let mut out = Vec::new();
    for (slot, count) in folded.slices {
        if slot == OTHERS {
            others += count;
        } else {
            out.push(Piece {
                slot,
                name: SLOT_NAMES.get(slot).copied().unwrap_or("others"),
                count,
            });
        }
    }
    if others > 0 {
        out.push(Piece {
            slot: OTHERS,
            name: "others",
            count: others,
        });
    }
    (out, total)
}

pub(super) fn kinds(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    let (pieces, total) = pieces(ctx);
    if total == 0 {
        note(buf, area, NO_COMMITS, ctx.dim());
        return;
    }
    if pieces.iter().all(|p| p.slot == OTHERS) {
        note(buf, area, "no conventional prefixes", ctx.dim());
        return;
    }
    let counts: Vec<u64> = pieces.iter().map(|p| p.count).collect();
    let split = shares(&counts);
    let colors = ctx.colors();
    let figures: Vec<String> = split
        .items
        .iter()
        .map(|s| figure(*s, split.total, ctx.view.show_counts))
        .collect();
    let name_w = pieces.iter().map(|p| p.name.len()).max().unwrap_or(0);
    let (columns, _) = figure_columns(&figures, ctx.dim());
    let legend: Vec<Line<'static>> = pieces
        .iter()
        .zip(columns)
        .map(|(p, figure)| {
            let mut spans = vec![
                Span::styled(
                    slot_marker(p.slot),
                    Style::new().fg(colors.slot_color(p.slot)),
                ),
                Span::raw(format!(" {:<name_w$}  ", p.name)),
            ];
            spans.extend(figure);
            Line::from(spans)
        })
        .collect();
    let ring_w = RING_WIDTH.min(area.width.saturating_sub(2 + LEGEND_MIN));
    let ring = Rect::new(area.x, area.y, ring_w, RING_HEIGHT.min(area.height));
    if ctx.view.mode == ChartMode::Braille && donut::fits(ring) {
        let slices: Vec<Slice> = pieces
            .iter()
            .map(|p| Slice {
                weight: p.count,
                color: colors.slot_color(p.slot),
            })
            .collect();
        Donut::new(&slices).render(ring, buf);
        // The legend sits beside the ring, vertically centred on it.
        let rows = u16::try_from(legend.len()).unwrap_or(u16::MAX);
        let top = area.y + ring.height.saturating_sub(rows) / 2;
        let x = area.x + ring_w + 2;
        let legend_area = Rect::new(
            x,
            top,
            area.right().saturating_sub(x),
            area.bottom().saturating_sub(top),
        );
        Paragraph::new(legend).render(legend_area, buf);
    } else {
        // Too small for a ring, or no Braille: one 100 % bar and the legend under it.
        let parts: Vec<_> = pieces
            .iter()
            .map(|p| (p.count, colors.slot_color(p.slot)))
            .collect();
        let mut lines = vec![stacked_bar(&parts, area.width)];
        lines.extend(legend);
        Paragraph::new(lines).render(area, buf);
    }
}

pub(super) fn heat_title(width: u16) -> String {
    format!("Commits per day ({} weeks)", heatmap::weeks_shown(width))
}

pub(super) fn heat(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
    if area.height < 2 {
        return;
    }
    let counts: Vec<(i64, u32)> = ctx
        .stats
        .daily
        .iter()
        .map(|b| {
            (
                b.start.div_euclid(DAY),
                u32::try_from(b.commits).unwrap_or(u32::MAX),
            )
        })
        .collect();
    let map_rows = if area.height > HEAT_FULL {
        HEAT_FULL
    } else {
        HEAT_SHORT.min(area.height - 1)
    };
    let map = Rect {
        height: map_rows,
        ..area
    };
    HeatMap::new(&counts, ctx.view.now.div_euclid(DAY), ctx.colors().heat).render(map, buf);
    if area.height > map_rows {
        let legend = Rect::new(
            area.x + 4,
            area.y + map_rows,
            area.width.saturating_sub(4),
            1,
        );
        Paragraph::new(heatmap::legend(ctx.colors().heat)).render(legend, buf);
    }
}
