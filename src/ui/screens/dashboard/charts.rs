//! The three chart sections of the dashboard: Activity (line chart), What was
//! done (donut) and Commits per day (heat map). Each draws into the area it is
//! given and degrades when it is small.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine, Points};
use ratatui::widgets::{Paragraph, Sparkline, Widget};

use super::text::{axis_date, figure, figure_columns, plural};
use super::{Ctx, note};
use crate::git::stats::series::{Bucket, Granularity};
use crate::git::stats::share::{fold, shares};
use crate::ui::widgets::chart_palette::{ChartMode, OTHERS, kind_slot, slot_marker};
use crate::ui::widgets::donut::{self, Donut, Slice};
use crate::ui::widgets::heatmap::{self, HeatMap};
use crate::ui::widgets::share_bar::stacked_bar;

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

pub(crate) const NO_COMMITS: &str = "no commits in this window";

fn count_f64(n: usize) -> f64 {
    u32::try_from(n).map_or_else(|_| f64::from(u32::MAX), f64::from)
}

/// `commits per day`, the dim words after the section title.
pub(crate) fn activity_unit(ctx: &Ctx<'_>) -> String {
    let per = match ctx.stats.granularity {
        Granularity::Day => "day",
        Granularity::Week => "week",
        Granularity::Month => "month",
    };
    format!("commits per {per}")
}

/// `26 days of history · peak 58 (09-04)`: the one caption under the plot.
fn activity_caption(ctx: &Ctx<'_>, peak: Bucket, long: bool) -> String {
    let unit = match ctx.stats.granularity {
        Granularity::Day => "days",
        Granularity::Week => "weeks",
        Granularity::Month => "months",
    };
    format!(
        "{} {unit} of history · peak {} ({})",
        ctx.stats.series.len(),
        peak.commits,
        axis_date(peak.start, ctx.stats.granularity, long)
    )
}

pub(crate) fn activity(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
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
    let Some(peak) = stats.series.iter().max_by_key(|b| b.commits) else {
        return;
    };
    if area.height < 2 {
        return;
    }
    let long = last.start - first.start > 300 * DAY;
    let caption = Rect::new(area.x, area.bottom() - 1, area.width, 1);
    let plot = Rect {
        height: area.height - 1,
        ..area
    };
    note(buf, caption, &activity_caption(ctx, *peak, long), ctx.dim());
    match ctx.view.mode {
        ChartMode::Braille => braille_line(ctx, plot, buf, peak.commits.max(1)),
        ChartMode::Blocks => blocks_line(ctx, plot, buf, peak.commits.max(1)),
    }
}

/// The line chart on a `Canvas`: no axis box, two dim tick labels (the peak on
/// the top row, 0 on the bottom one) and one dot at the last point.
fn braille_line(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer, peak: usize) {
    let series = &ctx.stats.series;
    let top = peak.to_string();
    let gutter = u16::try_from(top.len())
        .unwrap_or(u16::MAX)
        .saturating_add(1);
    if area.width <= gutter + 1 {
        return;
    }
    let mut ticks = |y: u16, text: &str| {
        note(buf, Rect::new(area.x, y, gutter, 1), text, ctx.dim());
    };
    ticks(area.y, &top);
    ticks(area.bottom() - 1, "0");
    let points: Vec<(f64, f64)> = series
        .iter()
        .enumerate()
        .map(|(i, b)| (count_f64(i), count_f64(b.commits)))
        .collect();
    let color = ctx.colors().accent;
    let end = points.last().copied().unwrap_or_default();
    Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([0.0, count_f64(series.len() - 1)])
        .y_bounds([0.0, count_f64(peak)])
        .paint(|c| {
            for pair in points.windows(2) {
                if let [(x1, y1), (x2, y2)] = *pair {
                    c.draw(&CanvasLine {
                        x1,
                        y1,
                        x2,
                        y2,
                        color,
                    });
                }
            }
            c.draw(&Points {
                coords: &[end],
                color,
            });
            c.print(end.0, end.1, Span::styled("●", Style::new().fg(color)));
        })
        .render(
            Rect::new(area.x + gutter, area.y, area.width - gutter, area.height),
            buf,
        );
}

/// The block-glyph fallback of the line chart: a `Sparkline` filling `area`.
fn blocks_line(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer, peak: usize) {
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
    Sparkline::default()
        .data(&data)
        .max(u64::try_from(peak).unwrap_or(1))
        .style(Style::new().fg(ctx.colors().accent))
        .render(area, buf);
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

pub(crate) fn kinds(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
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

/// `26 weeks`, the dim words after the section title.
pub(crate) fn heat_unit(width: u16) -> String {
    format!("{} weeks", heatmap::weeks_shown(width))
}

pub(crate) fn heat(ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
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
    let density = ctx.colors().density;
    HeatMap::new(&counts, ctx.view.now.div_euclid(DAY), ctx.colors().heat)
        .density(density)
        .render(map, buf);
    if area.height > map_rows {
        let legend = Rect::new(
            area.x + 4,
            area.y + map_rows,
            area.width.saturating_sub(4),
            1,
        );
        Paragraph::new(heatmap::legend(ctx.colors().heat, density, ctx.dim())).render(legend, buf);
    }
}
