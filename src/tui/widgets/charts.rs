//! charts.rs

use crate::git::stats::kind::Kind;
use crate::theme::palette::Palette;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;
use ratatui::widgets::canvas::{Canvas, Painter, Shape};
use std::collections::HashMap;
use std::f64::consts::TAU;

/// Smallest area (columns) that draws a ring; below it use a share bar.
pub(crate) const MIN_WIDTH: u16 = 12;
/// Smallest area (rows) that draws a ring; below it use a share bar.
pub(crate) const MIN_HEIGHT: u16 = 6;

/// Inner radius as a share of the outer one: the ring is a quarter of the radius thick.
const INNER: f64 = 0.75;
/// Half the gap between two slices, in dots along the arc: dots within it of a
/// boundary are left blank, so the gap is about one dot wide (two when the boundary
/// falls between two dot columns, as at 12 o'clock).
const HALF_GAP: f64 = 0.6;

/// One slice of the donut: its weight and its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Slice {
    /// Share of the whole, in any unit; only the ratio to the other weights counts.
    pub weight: u64,
    /// Colour of every cell that this slice covers the most.
    pub color: Color,
}

/// The donut widget. Draws nothing when there is no weight or the area is too small.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub(crate) struct Donut<'a> {
    slices: &'a [Slice],
}

impl<'a> Donut<'a> {
    pub(crate) fn new(slices: &'a [Slice]) -> Self {
        Self { slices }
    }
}

/// True when `area` is big enough for a ring (`MIN_WIDTH` x `MIN_HEIGHT`).
pub(crate) fn fits(area: Rect) -> bool {
    area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT
}

/// Index of the slice under `angle` (degrees, from 12 o'clock, clockwise; any value
/// wraps into one turn). A boundary belongs to the next slice. `None` when the
/// total weight is zero.
#[allow(clippy::cast_precision_loss)] // weights are counts, far below 2^53
pub(crate) fn slice_at(angle: f64, slices: &[Slice]) -> Option<usize> {
    let total: u64 = slices.iter().map(|s| s.weight).sum();
    if total == 0 {
        return None;
    }
    let target = (angle / 360.0).rem_euclid(1.0) * total as f64;
    let mut acc = 0.0;
    for (i, s) in slices.iter().enumerate() {
        acc += s.weight as f64;
        if s.weight > 0 && target < acc {
            return Some(i);
        }
    }
    // Rounding pushed `target` to the very end: the last slice with weight.
    slices.iter().rposition(|s| s.weight > 0)
}

/// Side of the dot grid for `area`: the smaller dimension in dots, a multiple of 4
/// so it is a whole number of cells both ways.
fn dots_for(area: Rect) -> u32 {
    (u32::from(area.width) * 2).min(u32::from(area.height) * 4) / 4 * 4
}

/// A cell count that came from an area's `u16` size, so it fits again.
fn area_cells(n: u32) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// The ring on a `dots` x `dots` grid.
struct Ring<'a> {
    slices: &'a [Slice],
    dots: u32,
}

impl Ring<'_> {
    /// Slice of the dot at (`x`, `y`) counted from the top left, or `None` when the
    /// dot is off the ring or in the gap between two slices.
    fn dot(&self, x: u32, y: u32) -> Option<usize> {
        let radius = f64::from(self.dots) / 2.0;
        let dx = f64::from(x) + 0.5 - radius;
        let dy = f64::from(y) + 0.5 - radius;
        let r = dx.hypot(dy);
        if r > radius || r < radius * INNER {
            return None;
        }
        let turns = dx.atan2(-dy).rem_euclid(TAU) / TAU;
        if self.slices.iter().filter(|s| s.weight > 0).count() >= 2 {
            let total: f64 = self.slices.iter().map(|s| weight_f64(s.weight)).sum();
            let mut edge = 0.0;
            for s in self.slices.iter().filter(|s| s.weight > 0) {
                let d = (turns - edge).abs();
                if d.min(1.0 - d) * TAU * r < HALF_GAP {
                    return None;
                }
                edge += weight_f64(s.weight) / total;
            }
        }
        slice_at(turns * 360.0, self.slices)
    }
}

#[allow(clippy::cast_precision_loss)] // weights are counts, far below 2^53
fn weight_f64(weight: u64) -> f64 {
    weight as f64
}

impl Shape for Ring<'_> {
    fn draw(&self, painter: &mut Painter<'_, '_>) {
        // One foreground colour per cell: the slice with most dots in it wins.
        for cy in 0..self.dots / 4 {
            for cx in 0..self.dots / 2 {
                let lit: Vec<(u32, u32, usize)> = (cy * 4..cy * 4 + 4)
                    .flat_map(|y| (cx * 2..cx * 2 + 2).map(move |x| (x, y)))
                    .filter_map(|(x, y)| self.dot(x, y).map(|i| (x, y, i)))
                    .collect();
                let winner = lit
                    .iter()
                    .map(|&(.., i)| i)
                    .max_by_key(|&i| lit.iter().filter(|&&(.., j)| j == i).count());
                let Some(color) = winner.and_then(|i| self.slices.get(i)).map(|s| s.color) else {
                    continue;
                };
                for (x, y, _) in lit {
                    painter.paint(x as usize, y as usize, color);
                }
            }
        }
    }
}

impl Widget for Donut<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        let dots = dots_for(area);
        if !fits(area) || self.slices.iter().all(|s| s.weight == 0) {
            return;
        }
        let side = f64::from(dots - 1);
        // The canvas is the centred block of whole cells that holds the square grid.
        let (w, h) = (area_cells(dots / 2), area_cells(dots / 4));
        let canvas = Rect::new(
            area.x + (area.width - w) / 2,
            area.y + (area.height - h) / 2,
            w,
            h,
        );
        let ring = Ring {
            slices: self.slices,
            dots,
        };
        Canvas::default()
            .marker(Marker::Braille)
            .x_bounds([0.0, side])
            .y_bounds([0.0, side])
            .paint(|ctx| ctx.draw(&ring))
            .render(canvas, buf);
    }
}

/// The glyph of each level without colour: none, then four buckets of non-zero days.
const DENSITY: [&str; 5] = ["·", "░", "▒", "▓", "█"];
/// The one glyph of every level when colour carries it.
const SQUARE: &str = "■";
/// Cells reserved on the left for the weekday labels.
const GUTTER: u16 = 4;
/// Cells per week column (one glyph and one space).
const CELL: u16 = 2;
/// The most weeks ever drawn.
const MAX_WEEKS: u16 = 26;
/// Height from which all seven weekdays are drawn (header row included).
const FULL_HEIGHT: u16 = 9;
/// A week label goes over every this many columns, counted from the oldest.
const LABEL_EVERY: u16 = 4;

/// The weekday of a unix day number, Monday = 0 to Sunday = 6 (day 0 was a Thursday).
pub(crate) fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

/// The three quartile cut-offs (nearest rank) of the non-zero `counts`.
///
/// Zeros are ignored. With no non-zero count every cut-off is 0.
pub(crate) fn thresholds(counts: &[u32]) -> [u32; 3] {
    let mut sorted: Vec<u32> = counts.iter().copied().filter(|&c| c > 0).collect();
    sorted.sort_unstable();
    let n = sorted.len();
    let at = |quarter: usize| {
        let rank = (n * quarter).div_ceil(4).saturating_sub(1);
        sorted.get(rank).copied().unwrap_or(0)
    };
    [at(1), at(2), at(3)]
}

/// The level of `count`, 0 for no commit, 1 to 4 by the quartile `thresholds`.
pub(crate) fn level(count: u32, thresholds: [u32; 3]) -> usize {
    if count == 0 {
        return 0;
    }
    1 + thresholds.iter().filter(|&&t| count > t).count()
}

/// The glyph of `level` in the colour ramp (`density` false) or density mode.
fn glyph(level: usize, density: bool) -> &'static str {
    if density {
        DENSITY.get(level).copied().unwrap_or("·")
    } else {
        SQUARE
    }
}

/// The `less ■ ■ ■ ■ ■ more` legend line, every level in its own style; `less`
/// and `more` are `label`.
pub(crate) fn legend(styles: [Style; 5], density: bool, label: Style) -> Line<'static> {
    let mut spans = vec![Span::styled("less", label)];
    for (level, style) in styles.into_iter().enumerate() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(glyph(level, density), style));
    }
    spans.push(Span::styled(" more", label));
    Line::from(spans)
}

/// Week columns drawn in a panel `width` cells wide (the title says how many).
pub(crate) fn weeks_shown(width: u16) -> u16 {
    (width.saturating_sub(GUTTER) / CELL).min(MAX_WEEKS)
}

/// A commits-per-day heat map.
///
/// Levels come from the quartiles of the non-zero days shown, so one burst day does
/// not flatten the others. Days after `today` stay blank. The header labels every
/// fourth column `W1`, `W5`, ... counting weeks from the oldest one shown.
#[derive(Debug, Clone)]
pub(crate) struct HeatMap<'a> {
    counts: &'a [(i64, u32)],
    today: i64,
    styles: [Style; 5],
    density: bool,
}

impl<'a> HeatMap<'a> {
    /// `counts` are `(unix day, commits)`, in any order; a repeated day is summed.
    /// `styles[0]` styles a quiet day, `styles[1..]` the four buckets.
    pub(crate) fn new(counts: &'a [(i64, u32)], today: i64, styles: [Style; 5]) -> Self {
        Self {
            counts,
            today,
            styles,
            density: false,
        }
    }

    /// Draw the level as a glyph (`· ░ ▒ ▓ █`) instead of a coloured `■`.
    #[must_use]
    pub(crate) const fn density(mut self, density: bool) -> Self {
        self.density = density;
        self
    }
}

impl Widget for HeatMap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let weeks = weeks_shown(area.width);
        if area.is_empty() || weeks == 0 {
            return;
        }
        let first_monday = self.today - weekday(self.today) - 7 * i64::from(weeks - 1);
        let mut per_day: HashMap<i64, u32> = HashMap::new();
        for &(day, n) in self.counts {
            *per_day.entry(day).or_default() += n;
        }
        let shown: Vec<u32> = per_day
            .iter()
            .filter(|&(&d, _)| (first_monday..=self.today).contains(&d))
            .map(|(_, &n)| n)
            .collect();
        let cuts = thresholds(&shown);

        let rows: &[(i64, &str)] = if area.height >= FULL_HEIGHT {
            &[
                (0, "Mon"),
                (1, "Tue"),
                (2, "Wed"),
                (3, "Thu"),
                (4, "Fri"),
                (5, "Sat"),
                (6, "Sun"),
            ]
        } else {
            &[(0, "Mon"), (2, "Wed"), (4, "Fri")]
        };
        let grid_x = area.x.saturating_add(GUTTER);
        let quiet = Style::new().add_modifier(Modifier::DIM);
        let mut put = |x: u16, y: u16, text: &str, style: Style| {
            if let Some(cell) = buf
                .cell_mut((x, y))
                .filter(|_| area.contains((x, y).into()))
            {
                cell.set_symbol(text).set_style(style);
            }
        };

        for col in 0..weeks {
            if col % LABEL_EVERY == 0 {
                let label = format!("W{}", col + 1);
                let x = grid_x.saturating_add(col * CELL);
                let fits = usize::from(x) + label.len() <= usize::from(area.right());
                if fits {
                    for (dx, ch) in (0..).zip(label.chars()) {
                        put(x.saturating_add(dx), area.y, &ch.to_string(), quiet);
                    }
                }
            }
        }
        for (&(wd, name), y) in rows.iter().zip(area.y.saturating_add(1)..area.bottom()) {
            for (dx, ch) in (0..).zip(name.chars()) {
                put(area.x.saturating_add(dx), y, &ch.to_string(), quiet);
            }
            for col in 0..weeks {
                let day = first_monday + 7 * i64::from(col) + wd;
                if day > self.today {
                    continue;
                }
                let lvl = level(per_day.get(&day).copied().unwrap_or(0), cuts);
                let style = self.styles.get(lvl).copied().unwrap_or_default();
                let x = grid_x.saturating_add(col * CELL);
                put(x, y, glyph(lvl, self.density), style);
            }
        }
    }
}

const FULL: &str = "━";
const EMPTY: &str = "─";

fn dim() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

/// Filled cells for `fraction` of `width`: nearest cell, at least one for a
/// non-zero share, never the whole bar for a share under 100 % (both rules
/// need a width of 2 or more; at width 1 the plain rounding wins).
fn filled_cells(fraction: f64, width: u16) -> u16 {
    if width == 0 || !fraction.is_finite() || fraction <= 0.0 {
        return 0;
    }
    if fraction >= 1.0 {
        return width;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "fraction is in (0, 1), so the product is in [0, width]"
    )]
    let rounded = (fraction * f64::from(width)).round() as u16;
    if width < 2 {
        return rounded;
    }
    rounded.clamp(1, width - 1)
}

fn run(glyph: &str, cells: u16, style: Style) -> Option<Span<'static>> {
    (cells > 0).then(|| Span::styled(glyph.repeat(usize::from(cells)), style))
}

/// One share as `━` (in `fg`) followed by the dim `─` track, exactly `width` cells.
pub(crate) fn single_bar(fraction: f64, width: u16, fg: Color) -> Vec<Span<'static>> {
    let filled = filled_cells(fraction, width);
    [
        run(FULL, filled, Style::new().fg(fg)),
        run(EMPTY, width - filled, dim()),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Cells per part, by largest remainder, adding up to `width` (all zero when
/// the total is zero). Every non-zero part gets a cell when the width allows.
fn split_cells(parts: &[(u64, Color)], width: u16) -> Vec<u16> {
    let total: u128 = parts.iter().map(|&(w, _)| u128::from(w)).sum();
    if total == 0 || width == 0 {
        return vec![0; parts.len()];
    }
    let width_wide = u128::from(width);
    let mut cells: Vec<u16> = Vec::with_capacity(parts.len());
    let mut remainders: Vec<(usize, u128)> = Vec::with_capacity(parts.len());
    for (i, &(w, _)) in parts.iter().enumerate() {
        let scaled = u128::from(w) * width_wide;
        cells.push(u16::try_from(scaled / total).unwrap_or(width));
        remainders.push((i, scaled % total));
    }
    // Largest remainder first; ties go to the earlier part.
    remainders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let given: u16 = cells.iter().sum();
    for &(i, _) in remainders.iter().take(usize::from(width - given)) {
        if let Some(c) = cells.get_mut(i) {
            *c += 1;
        }
    }
    // A tiny part must stay visible: take the cell from the widest part.
    let non_zero = parts.iter().filter(|&&(w, _)| w > 0).count();
    if usize::from(width) >= non_zero {
        for i in 0..parts.len() {
            let needs = parts.get(i).is_some_and(|&(w, _)| w > 0) && cells.get(i) == Some(&0);
            if !needs {
                continue;
            }
            let donor = cells
                .iter()
                .enumerate()
                .max_by_key(|&(j, &c)| (c, std::cmp::Reverse(j)))
                .map(|(j, _)| j);
            if let Some(d) = donor.and_then(|d| cells.get_mut(d)) {
                *d -= 1;
            }
            if let Some(c) = cells.get_mut(i) {
                *c = 1;
            }
        }
    }
    cells
}

/// One 100 % line split between `parts` (weight, colour), exactly `width`
/// cells. A zero total gives a dim `─` line.
pub(crate) fn stacked_bar(parts: &[(u64, Color)], width: u16) -> Line<'static> {
    let cells = split_cells(parts, width);
    if cells.iter().all(|&c| c == 0) {
        return Line::from(run(EMPTY, width, dim()).into_iter().collect::<Vec<_>>());
    }
    Line::from(
        parts
            .iter()
            .zip(cells)
            .filter_map(|(&(_, color), c)| run(FULL, c, Style::new().fg(color)))
            .collect::<Vec<_>>(),
    )
}

/// The figure next to a bar: percentage first (`41 %  (124)`), or the count
/// first (`124  (41 %)`) when `show_counts`. `percent` is `None` for a share
/// under 1 %; `None` with a zero count means there is no whole, shown as `–`.
pub(crate) fn percent_label(percent: Option<u8>, count: u64, show_counts: bool) -> String {
    if percent.is_none() && count == 0 {
        return "–".to_owned();
    }
    let pct = percent.map_or_else(|| "<1 %".to_owned(), |p| format!("{p} %"));
    if show_counts {
        format!("{count}  ({pct})")
    } else {
        format!("{pct}  ({count})")
    }
}

/// Right-aligns `label` in a field of `width` characters (never truncates).
pub(crate) fn pad_label(label: &str, width: usize) -> String {
    format!("{label:>width$}")
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "test scaffolding: an out-of-range index is the failed assertion"
)]
mod tests_share_bar {
    use super::*;

    fn text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn filled(spans: &[Span<'_>]) -> usize {
        text(spans).chars().filter(|&c| c == '━').count()
    }

    fn line_cells(line: &Line<'_>) -> usize {
        line.spans.iter().map(|s| s.content.chars().count()).sum()
    }

    #[test]
    fn single_bar_widths() {
        for (fraction, width, want) in [
            (0.0, 10, 0),
            (0.01, 10, 1),
            (0.5, 10, 5),
            (0.99, 10, 9),
            (1.0, 10, 10),
            (0.5, 0, 0),
            (1.0, 0, 0),
            (1.0, 1, 1),
            (0.0, 1, 0),
            (0.5, 1, 1),
        ] {
            let bar = single_bar(fraction, width, Color::Green);
            assert_eq!(text(&bar).chars().count(), usize::from(width));
            assert_eq!(filled(&bar), want, "{fraction} of {width}");
        }
    }

    #[test]
    fn single_bar_rules() {
        assert_eq!(filled(&single_bar(0.001, 20, Color::Red)), 1);
        assert_eq!(filled(&single_bar(0.999, 20, Color::Red)), 19);
        assert_eq!(filled(&single_bar(0.001, 2, Color::Red)), 1);
        assert_eq!(filled(&single_bar(-1.0, 8, Color::Red)), 0);
        assert_eq!(filled(&single_bar(7.0, 8, Color::Red)), 8);
        assert_eq!(filled(&single_bar(f64::NAN, 8, Color::Red)), 0);
    }

    #[test]
    fn single_bar_colours_and_dim() {
        let bar = single_bar(0.5, 4, Color::Blue);
        assert_eq!(bar.len(), 2);
        assert_eq!(bar[0].style.fg, Some(Color::Blue));
        assert_eq!(bar[0].content, "━━");
        assert_eq!(bar[1].style.fg, None);
        assert!(bar[1].style.add_modifier.contains(Modifier::DIM));
        assert_eq!(bar[1].content, "──");
    }

    #[test]
    fn stacked_bar_adds_up_to_width() {
        let colors = [Color::Red, Color::Green, Color::Blue, Color::Yellow];
        for width in [1u16, 2, 3, 7, 10, 33, 80] {
            for weights in [
                vec![1u64, 1, 1],
                vec![3, 3, 3, 1],
                vec![97, 2, 1],
                vec![u64::MAX, u64::MAX, 1],
                vec![5, 0, 7],
            ] {
                let parts: Vec<_> = weights
                    .iter()
                    .copied()
                    .zip(colors.into_iter().cycle())
                    .collect();
                let line = stacked_bar(&parts, width);
                assert_eq!(
                    line_cells(&line),
                    usize::from(width),
                    "{weights:?} in {width}"
                );
            }
        }
    }

    #[test]
    fn stacked_bar_tiny_part_is_visible() {
        let parts = [
            (10_000, Color::Green),
            (1, Color::Red),
            (5_000, Color::Blue),
        ];
        let line = stacked_bar(&parts, 20);
        assert_eq!(line_cells(&line), 20);
        let red = line.spans.iter().find(|s| s.style.fg == Some(Color::Red));
        assert_eq!(red.map(|s| s.content.chars().count()), Some(1));
    }

    #[test]
    fn stacked_bar_colours_follow_parts() {
        let line = stacked_bar(
            &[(1, Color::Green), (0, Color::Yellow), (1, Color::Red)],
            10,
        );
        let got: Vec<_> = line
            .spans
            .iter()
            .map(|s| (s.style.fg, s.content.chars().count()))
            .collect();
        assert_eq!(got, [(Some(Color::Green), 5), (Some(Color::Red), 5)]);
    }

    #[test]
    fn stacked_bar_zero_total_is_dim() {
        for parts in [&[][..], &[(0, Color::Red), (0, Color::Blue)][..]] {
            let line = stacked_bar(parts, 6);
            assert_eq!(line.spans.len(), 1);
            assert_eq!(line.spans[0].content, "──────");
            assert!(line.spans[0].style.add_modifier.contains(Modifier::DIM));
        }
        assert_eq!(line_cells(&stacked_bar(&[(1, Color::Red)], 0)), 0);
    }

    #[test]
    fn stacked_bar_narrower_than_parts() {
        let parts = [(5, Color::Red), (3, Color::Green), (2, Color::Blue)];
        let line = stacked_bar(&parts, 2);
        assert_eq!(line_cells(&line), 2);
        assert_eq!(line_cells(&stacked_bar(&parts, 1)), 1);
    }

    #[test]
    fn labels() {
        assert_eq!(percent_label(Some(41), 124, false), "41 %  (124)");
        assert_eq!(percent_label(Some(41), 124, true), "124  (41 %)");
        assert_eq!(percent_label(None, 3, false), "<1 %  (3)");
        assert_eq!(percent_label(None, 3, true), "3  (<1 %)");
        assert_eq!(percent_label(None, 0, false), "–");
        assert_eq!(percent_label(None, 0, true), "–");
        assert_eq!(percent_label(Some(100), 9, false), "100 %  (9)");
    }

    #[test]
    fn padding() {
        assert_eq!(pad_label("41 %", 8), "    41 %");
        assert_eq!(pad_label("–", 3), "  –");
        assert_eq!(pad_label("100 %  (9)", 4), "100 %  (9)");
    }
}

/// Categorical slots: feat, fix, docs, test, refactor, then gray "others".
pub const SLOTS: usize = 6;
/// The gray "others" slot.
pub const OTHERS: usize = SLOTS - 1;
/// One shape per categorical slot, so colour is never the only signal.
const MARKERS: [&str; SLOTS] = ["●", "■", "▲", "◆", "▼", "○"];

/// Dark variants of the colours that wash out on white, and the gray.
const LIGHT_YELLOW: Color = Color::Rgb(154, 103, 0);
const LIGHT_CYAN: Color = Color::Rgb(14, 116, 144);
const LIGHT_GRAY: Color = Color::Rgb(107, 114, 128);

/// How the charts are drawn: Braille dots, or block glyphs for a terminal
/// without Braille (the Linux console, a non-UTF-8 locale).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartMode {
    Braille,
    Blocks,
}

/// `auto`: Braille, unless the locale is not UTF-8 or `TERM=linux`. `locale` is
/// the first set of `LC_ALL`, `LC_CTYPE`, `LANG`; an unset locale is not held
/// against the terminal.
pub fn charts_mode_auto(locale: Option<&str>, term: Option<&str>) -> ChartMode {
    let utf8 = locale
        .filter(|l| !l.is_empty())
        .is_none_or(|l| l.to_ascii_lowercase().replace('-', "").contains("utf8"));
    if utf8 && term != Some("linux") {
        ChartMode::Braille
    } else {
        ChartMode::Blocks
    }
}

/// `charts_mode_auto` on this process's environment.
pub fn charts_mode_from_env() -> ChartMode {
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()));
    charts_mode_auto(locale.as_deref(), std::env::var("TERM").ok().as_deref())
}

/// The brighter variant of an ANSI colour; anything else stays as it is.
fn bright(color: Color) -> Color {
    match color {
        Color::Black => Color::DarkGray,
        Color::Red => Color::LightRed,
        Color::Green => Color::LightGreen,
        Color::Yellow => Color::LightYellow,
        Color::Blue => Color::LightBlue,
        Color::Magenta => Color::LightMagenta,
        Color::Cyan => Color::LightCyan,
        Color::Gray => Color::White,
        other => other,
    }
}

/// Everything the dashboard colours with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartPalette {
    /// feat, fix, docs, test, refactor, others: the kinds of change only, authors
    /// and files are one series and wear the accent.
    pub categorical: [Color; SLOTS],
    /// Line chart, single-series bars.
    pub accent: Color,
    /// Lines added and removed.
    pub add: Color,
    pub del: Color,
    /// The `↑` ahead arrow and the `stale` alert.
    pub warn: Color,
    /// Quiet day, then four levels from dim to bright.
    pub heat: [Style; 5],
    /// `NO_COLOR`: the heat map's level is a glyph (`· ░ ▒ ▓ █`), not only a colour.
    pub density: bool,
    pub branch_current: Style,
    pub branch_active: Style,
    pub branch_merged: Style,
    pub branch_stale: Style,
    /// Secondary figures (counts in brackets), empty bar cells, captions.
    pub dim: Style,
    /// The page border and the thin rule under each section title: `Palette.idle`, dim.
    pub rule: Style,
}

impl ChartPalette {
    pub fn for_palette(p: &Palette) -> Self {
        let (yellow, cyan, gray) = if p.light {
            (LIGHT_YELLOW, LIGHT_CYAN, LIGHT_GRAY)
        } else {
            (Color::Yellow, Color::Cyan, Color::Gray)
        };
        let dim = Style::new().add_modifier(Modifier::DIM);
        let accent = Style::new().fg(p.focus);
        Self {
            categorical: [
                Color::Green,
                yellow,
                Color::Blue,
                cyan,
                Color::Magenta,
                gray,
            ],
            accent: p.focus,
            add: p.add,
            del: p.del,
            warn: p.warn,
            heat: [
                Style::new().fg(p.idle).add_modifier(Modifier::DIM),
                accent.add_modifier(Modifier::DIM),
                accent,
                accent.add_modifier(Modifier::BOLD),
                Style::new()
                    .fg(bright(p.focus))
                    .add_modifier(Modifier::BOLD),
            ],
            density: false,
            branch_current: accent.add_modifier(Modifier::BOLD),
            branch_active: Style::new(),
            branch_merged: Style::new().fg(gray),
            branch_stale: Style::new().fg(p.warn),
            dim,
            rule: Style::new().fg(p.idle).add_modifier(Modifier::DIM),
        }
    }

    /// Colour of categorical slot `slot`; past the last it is the "others" gray.
    pub fn slot_color(&self, slot: usize) -> Color {
        self.categorical
            .get(slot.min(OTHERS))
            .copied()
            .unwrap_or(Color::Gray)
    }

    pub fn kind_color(&self, kind: Kind) -> Color {
        self.slot_color(kind_slot(kind))
    }
}

/// The categorical slot of a kind: the five named ones, every other kind is
/// "others".
pub const fn kind_slot(kind: Kind) -> usize {
    match kind {
        Kind::Feat => 0,
        Kind::Fix => 1,
        Kind::Docs => 2,
        Kind::Test => 3,
        Kind::Refactor => 4,
        _ => OTHERS,
    }
}

/// The legend marker of categorical slot `slot` (`○` for others and beyond).
pub fn slot_marker(slot: usize) -> &'static str {
    MARKERS.get(slot.min(OTHERS)).copied().unwrap_or("○")
}

pub fn kind_marker(kind: Kind) -> &'static str {
    slot_marker(kind_slot(kind))
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "test scaffolding: an out-of-range index is the failed assertion"
)]
mod tests_chart_palette {
    use super::*;

    #[test]
    fn dark_uses_ansi_names() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        assert_eq!(
            c.categorical,
            [
                Color::Green,
                Color::Yellow,
                Color::Blue,
                Color::Cyan,
                Color::Magenta,
                Color::Gray
            ]
        );
        assert_eq!(c.accent, Palette::DARK.focus);
        assert_eq!(c.add, Palette::DARK.add);
        assert_eq!(c.del, Palette::DARK.del);
    }

    #[test]
    fn light_uses_explicit_rgb_for_the_colours_that_wash_out() {
        let c = ChartPalette::for_palette(&Palette::LIGHT);
        assert!(matches!(c.slot_color(1), Color::Rgb(..)));
        assert!(matches!(c.slot_color(3), Color::Rgb(..)));
        assert_eq!(c.slot_color(0), Color::Green);
        assert_eq!(c.slot_color(2), Color::Blue);
    }

    #[test]
    fn a_theme_change_recolours_the_charts() {
        let mut p = Palette::DARK;
        p.focus = Color::Blue;
        p.warn = Color::Red;
        let c = ChartPalette::for_palette(&p);
        assert_eq!(c.accent, Color::Blue);
        assert_eq!(c.heat[2].fg, Some(Color::Blue));
        assert_eq!(c.heat[4].fg, Some(Color::LightBlue));
        assert_eq!(c.branch_current.fg, Some(Color::Blue));
        assert_eq!(c.branch_stale.fg, Some(Color::Red));
    }

    #[test]
    fn the_heat_ramp_goes_from_dim_to_bright() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        assert!(c.heat[0].add_modifier.contains(Modifier::DIM));
        assert!(c.heat[1].add_modifier.contains(Modifier::DIM));
        assert!(!c.heat[2].add_modifier.contains(Modifier::DIM));
        assert!(c.heat[3].add_modifier.contains(Modifier::BOLD));
        assert_eq!(c.heat[4].fg, Some(Color::LightGreen));
    }

    #[test]
    fn branch_states() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        assert!(c.branch_current.add_modifier.contains(Modifier::BOLD));
        assert_eq!(c.branch_active, Style::new());
        assert_eq!(c.branch_merged.fg, Some(Color::Gray));
        assert_eq!(c.branch_stale.fg, Some(Palette::DARK.warn));
    }

    #[test]
    fn the_five_named_kinds_have_their_own_colour_and_marker_the_rest_are_others() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        let named = [
            Kind::Feat,
            Kind::Fix,
            Kind::Docs,
            Kind::Test,
            Kind::Refactor,
        ];
        for (i, &a) in named.iter().enumerate() {
            for &b in &named[i + 1..] {
                assert_ne!(c.kind_color(a), c.kind_color(b));
                assert_ne!(kind_marker(a), kind_marker(b));
            }
            assert_ne!(c.kind_color(a), c.slot_color(OTHERS));
            assert_ne!(kind_marker(a), slot_marker(OTHERS));
        }
        for kind in [
            Kind::Perf,
            Kind::Style,
            Kind::Build,
            Kind::Ci,
            Kind::Chore,
            Kind::Other,
        ] {
            assert_eq!(c.kind_color(kind), c.slot_color(OTHERS));
            assert_eq!(kind_marker(kind), "○");
        }
        assert_eq!(kind_marker(Kind::Feat), "●");
    }

    #[test]
    fn the_rule_is_the_idle_colour_dimmed() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        assert_eq!(c.rule.fg, Some(Palette::DARK.idle));
        assert!(c.rule.add_modifier.contains(Modifier::DIM));
        assert_eq!(slot_marker(40), "○");
    }

    #[test]
    fn braille_unless_the_locale_is_not_utf8_or_the_console_is_linux() {
        use ChartMode::{Blocks, Braille};
        assert_eq!(
            charts_mode_auto(Some("en_US.UTF-8"), Some("xterm")),
            Braille
        );
        assert_eq!(charts_mode_auto(Some("C.utf8"), None), Braille);
        assert_eq!(charts_mode_auto(None, Some("xterm-256color")), Braille);
        assert_eq!(charts_mode_auto(Some(""), None), Braille);
        assert_eq!(charts_mode_auto(Some("C"), Some("xterm")), Blocks);
        assert_eq!(charts_mode_auto(Some("POSIX"), None), Blocks);
        assert_eq!(charts_mode_auto(Some("en_US.ISO-8859-1"), None), Blocks);
        assert_eq!(charts_mode_auto(Some("en_US.UTF-8"), Some("linux")), Blocks);
    }
}

#[cfg(test)]
mod tests_donut {
    use super::*;

    const RED: Color = Color::Red;
    const BLUE: Color = Color::Blue;
    const GREEN: Color = Color::Green;

    fn s(weight: u64, color: Color) -> Slice {
        Slice { weight, color }
    }

    fn render(slices: &[Slice], area: Rect, buf_area: Rect) -> Buffer {
        let mut buf = Buffer::empty(buf_area);
        Donut::new(slices).render(area, &mut buf);
        buf
    }

    fn lit(buf: &Buffer) -> Vec<(u16, u16, Color)> {
        let mut v = Vec::new();
        for y in buf.area.top()..buf.area.bottom() {
            for x in buf.area.left()..buf.area.right() {
                if let Some(c) = buf.cell((x, y))
                    && c.symbol() != " "
                {
                    v.push((x, y, c.fg));
                }
            }
        }
        v
    }

    /// The 8 dot bits of a Braille cell, `None` for anything else.
    fn braille_bits(symbol: &str) -> Option<u32> {
        let ch = u32::from(symbol.chars().next()?);
        (0x2800..0x2900).contains(&ch).then(|| ch - 0x2800)
    }

    fn dots_in(buf: &Buffer, pred: impl Fn(u16, u16) -> bool) -> u32 {
        let mut n = 0;
        for y in buf.area.top()..buf.area.bottom() {
            for x in buf.area.left()..buf.area.right() {
                if let Some(c) = buf.cell((x, y))
                    && pred(x, y)
                    && let Some(bits) = braille_bits(c.symbol())
                {
                    n += bits.count_ones();
                }
            }
        }
        n
    }

    #[test]
    fn slice_at_boundaries_wrap_and_weights() {
        let sl = [s(1, RED), s(1, BLUE), s(2, GREEN)];
        assert_eq!(slice_at(0.0, &sl), Some(0));
        assert_eq!(slice_at(89.9, &sl), Some(0));
        assert_eq!(slice_at(90.0, &sl), Some(1));
        assert_eq!(slice_at(179.9, &sl), Some(1));
        assert_eq!(slice_at(180.0, &sl), Some(2));
        assert_eq!(slice_at(359.9, &sl), Some(2));
        assert_eq!(slice_at(360.0, &sl), Some(0));
        assert_eq!(slice_at(-10.0, &sl), Some(2));
        assert_eq!(slice_at(450.0, &sl), Some(1));
    }

    #[test]
    fn slice_at_skips_zero_weights_and_empty() {
        assert_eq!(slice_at(10.0, &[]), None);
        assert_eq!(slice_at(10.0, &[s(0, RED)]), None);
        assert_eq!(slice_at(10.0, &[s(0, RED), s(3, BLUE)]), Some(1));
    }

    #[test]
    fn one_slice_draws_a_full_ring_of_its_colour() {
        let area = Rect::new(0, 0, 16, 8);
        let buf = render(&[s(5, RED)], area, area);
        let cells = lit(&buf);
        assert!(cells.len() > 20);
        assert!(cells.iter().all(|&(.., c)| c == RED));
        // all four sides of the ring are present
        assert!(cells.iter().any(|&(x, y, _)| x == 0 && y == 4));
        assert!(cells.iter().any(|&(x, y, _)| x == 15 && y == 4));
        assert!(cells.iter().any(|&(x, y, _)| x == 8 && y == 0));
        assert!(cells.iter().any(|&(x, y, _)| x == 8 && y == 7));
        // and the hole is empty
        assert_eq!(
            buf.cell((8, 4)).map(ratatui::buffer::Cell::symbol),
            Some(" ")
        );
    }

    #[test]
    fn zero_total_and_no_slices_draw_nothing() {
        let area = Rect::new(0, 0, 16, 8);
        let blank = Buffer::empty(area);
        assert_eq!(render(&[], area, area), blank);
        assert_eq!(render(&[s(0, RED), s(0, BLUE)], area, area), blank);
    }

    #[test]
    fn two_slices_use_both_colours() {
        let area = Rect::new(0, 0, 16, 8);
        let cells = lit(&render(&[s(1, RED), s(1, BLUE)], area, area));
        assert!(cells.iter().any(|&(.., c)| c == RED));
        assert!(cells.iter().any(|&(.., c)| c == BLUE));
        // clockwise from the top: red on the right half, blue on the left
        assert!(cells.iter().any(|&(x, _, c)| x >= 12 && c == RED));
        assert!(cells.iter().any(|&(x, _, c)| x <= 3 && c == BLUE));
    }

    #[test]
    fn adjacent_slices_stay_apart_at_the_top() {
        // the two dot columns next to the 12 o'clock boundary are blank
        let area = Rect::new(0, 0, 16, 8);
        let buf = render(&[s(1, RED), s(1, BLUE)], area, area);
        let bits = |x| buf.cell((x, 0)).and_then(|c| braille_bits(c.symbol()));
        assert_eq!(bits(7).map(|b| b & 0xB8), Some(0));
        assert_eq!(bits(8).map(|b| b & 0x47), Some(0));
        assert_eq!(buf.cell((7, 0)).map(|c| c.fg), Some(BLUE));
        assert_eq!(buf.cell((8, 0)).map(|c| c.fg), Some(RED));
    }

    #[test]
    fn too_small_draws_nothing() {
        for (w, h) in [(11, 8), (16, 5), (1, 1), (0, 0)] {
            let area = Rect::new(0, 0, w, h);
            assert!(!fits(area));
            let big = Rect::new(0, 0, 20, 10);
            assert_eq!(render(&[s(1, RED)], area, big), Buffer::empty(big));
        }
        assert!(fits(Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT)));
        let min = Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT);
        assert!(!lit(&render(&[s(1, RED)], min, min)).is_empty());
    }

    #[test]
    fn ring_is_round_on_a_wide_area_too() {
        for area in [
            Rect::new(0, 0, 16, 8),
            Rect::new(0, 0, 60, 8),
            Rect::new(0, 0, 16, 30),
        ] {
            let buf = render(&[s(1, RED)], area, area);
            let cells = lit(&buf);
            let (minx, maxx) = (
                cells.iter().map(|c| c.0).min().unwrap_or(0),
                cells.iter().map(|c| c.0).max().unwrap_or(0),
            );
            let (miny, maxy) = (
                cells.iter().map(|c| c.1).min().unwrap_or(0),
                cells.iter().map(|c| c.1).max().unwrap_or(0),
            );
            // width in dots equals height in dots
            assert_eq!(
                (maxx - minx + 1) * 2,
                (maxy - miny + 1) * 4,
                "{area:?} is not square in dots"
            );
            let (mx, my) = (u32::from(minx + maxx), u32::from(miny + maxy));
            let left = dots_in(&buf, |x, _| u32::from(x) * 2 < mx);
            let right = dots_in(&buf, |x, _| u32::from(x) * 2 > mx);
            let top = dots_in(&buf, |_, y| u32::from(y) * 2 < my);
            let bottom = dots_in(&buf, |_, y| u32::from(y) * 2 > my);
            let total = left + right;
            assert!(
                left.abs_diff(right) * 20 <= total,
                "{area:?} {left} {right}"
            );
            assert!(
                top.abs_diff(bottom) * 20 <= total,
                "{area:?} {top} {bottom}"
            );
            assert!(left.abs_diff(top) * 10 <= total, "{area:?} {left} {top}");
        }
    }

    #[test]
    fn tiny_and_huge_areas_do_not_panic() {
        let one = Rect::new(0, 0, 1, 1);
        let _ = render(&[s(1, RED), s(2, BLUE)], one, one);
        let huge = Rect::new(0, 0, 400, 200);
        assert!(!lit(&render(&[s(1, RED), s(2, BLUE), s(3, GREEN)], huge, huge)).is_empty());
    }

    #[test]
    fn never_writes_outside_the_area() {
        let buf_area = Rect::new(0, 0, 40, 20);
        let area = Rect::new(5, 3, 20, 10);
        let buf = render(&[s(1, RED), s(1, BLUE)], area, buf_area);
        let cells = lit(&buf);
        assert!(!cells.is_empty());
        assert!(cells.iter().all(|&(x, y, _)| area.contains((x, y).into())));
        // an area bigger than the buffer is clipped, not a panic
        let _ = render(&[s(1, RED)], Rect::new(30, 15, 40, 20), buf_area);
    }
}

#[cfg(test)]
mod tests_heatmap {
    use super::*;

    /// 2024-01-01 was a Monday: unix day 19723.
    const MONDAY: i64 = 19723;

    fn styles() -> [Style; 5] {
        [
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
        ]
        .map(|c| Style::default().fg(c))
    }

    fn render(counts: &[(i64, u32)], today: i64, w: u16, h: u16) -> Buffer {
        let area = Rect::new(0, 0, w, h);
        let mut buf = Buffer::empty(area);
        HeatMap::new(counts, today, styles()).render(area, &mut buf);
        buf
    }

    fn sym(buf: &Buffer, x: u16, y: u16) -> String {
        buf.cell((x, y))
            .map_or_else(String::new, |c| c.symbol().to_owned())
    }

    fn row_text(buf: &Buffer, y: u16) -> String {
        (0..buf.area.width).map(|x| sym(buf, x, y)).collect()
    }

    #[test]
    fn weekday_of_known_days() {
        assert_eq!(weekday(0), 3, "1970-01-01 is a Thursday");
        assert_eq!(weekday(MONDAY), 0);
        assert_eq!(weekday(MONDAY + 6), 6, "a Sunday");
        assert_eq!(weekday(-1), 2, "before the epoch: Wednesday");
    }

    #[test]
    fn burst_day_does_not_flatten_the_others() {
        let cuts = thresholds(&[1, 1, 1, 2, 2, 2, 3, 100]);
        assert_eq!(cuts, [1, 2, 2]);
        assert_eq!(level(0, cuts), 0);
        assert_eq!(level(1, cuts), 1);
        assert_eq!(level(2, cuts), 2);
        assert_eq!(level(3, cuts), 4);
        assert_eq!(level(100, cuts), 4);
        assert_eq!(thresholds(&[]), [0, 0, 0]);
        assert_eq!(level(5, thresholds(&[5])), 1, "a lone day is the palest");
    }

    #[test]
    fn days_after_today_are_blank() {
        // today is a Wednesday: Thu..Sun of the last column stay blank.
        let today = MONDAY + 2;
        let buf = render(&[], today, 12, 12);
        let last = GUTTER + 2 * 3;
        for (y, want) in [(1, "■"), (2, "■"), (3, "■"), (4, " "), (7, " ")] {
            assert_eq!(sym(&buf, last, y), want, "row {y}");
        }
    }

    #[test]
    fn three_rows_below_nine_seven_from_nine() {
        let small = render(&[], MONDAY, 20, 8);
        let labels: Vec<String> = (1..8)
            .map(|y| row_text(&small, y)[..3].to_string())
            .collect();
        assert_eq!(
            labels.get(..3),
            Some(&["Mon".to_owned(), "Wed".to_owned(), "Fri".to_owned()][..])
        );
        assert_eq!(row_text(&small, 4).trim(), "");
        let full = render(&[], MONDAY, 20, 9);
        let labels: Vec<String> = (1..8)
            .map(|y| row_text(&full, y)[..3].to_string())
            .collect();
        assert_eq!(labels, ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
    }

    #[test]
    fn weeks_cap_at_26_and_fit_narrow_widths() {
        let wide = render(&[], MONDAY + 6, 200, 9);
        let dots = row_text(&wide, 1).matches('■').count();
        assert_eq!(dots, 26);
        let narrow = render(&[], MONDAY + 6, 4 + 2 * 5 + 1, 9);
        assert_eq!(row_text(&narrow, 1).matches('■').count(), 5);
        assert_eq!(
            row_text(&render(&[], MONDAY, 5, 9), 1).matches('■').count(),
            0
        );
    }

    #[test]
    fn last_column_is_the_week_of_today() {
        // today = Friday: the last column shows Mon..Fri, with today's count.
        let today = MONDAY + 4;
        let buf = render(&[(today, 3)], today, 4 + 2 * 3, 9);
        let last = GUTTER + 2 * 2;
        assert_eq!(sym(&buf, last, 5), "■", "Friday holds today's commits");
        assert_eq!(sym(&buf, last, 6), " ", "Saturday is in the future");
        assert_eq!(sym(&buf, last, 1), "■");
        let mut plain = Buffer::empty(Rect::new(0, 0, 10, 9));
        HeatMap::new(&[(today, 3)], today, styles())
            .density(true)
            .render(Rect::new(0, 0, 10, 9), &mut plain);
        assert_eq!(sym(&plain, last, 5), "░", "density: the level is the glyph");
        assert_eq!(sym(&plain, last, 1), "·");
    }

    #[test]
    fn styles_apply_per_level() {
        let counts: Vec<(i64, u32)> = (0..4)
            .map(|i| (MONDAY + i, 1 + u32::try_from(i).unwrap_or(0)))
            .collect();
        let today = MONDAY + 6;
        let buf = render(&counts, today, 4 + 2, 9);
        let fg = |y: u16| buf.cell((GUTTER, y)).and_then(|c| c.style().fg);
        assert_eq!(fg(1), Some(Color::Green));
        assert_eq!(fg(2), Some(Color::Yellow));
        assert_eq!(fg(3), Some(Color::Blue));
        assert_eq!(fg(4), Some(Color::Magenta));
        assert_eq!(fg(5), Some(Color::Red), "a quiet day");
        assert_eq!(sym(&buf, GUTTER, 4), "■");
        assert_ne!(fg(1), fg(2), "one glyph, a colour per level");
    }

    #[test]
    fn empty_data_draws_the_quiet_grid_and_zero_area_nothing() {
        let buf = render(&[], MONDAY + 6, 20, 9);
        assert!(row_text(&buf, 3).contains('■'));
        let none = Buffer::empty(Rect::new(0, 0, 10, 5));
        let mut buf = none.clone();
        HeatMap::new(&[], MONDAY, styles()).render(Rect::new(0, 0, 0, 5), &mut buf);
        HeatMap::new(&[], MONDAY, styles()).render(Rect::new(0, 0, 10, 0), &mut buf);
        assert_eq!(buf, none);
    }

    #[test]
    fn nothing_is_written_outside_the_area() {
        let outer = Rect::new(0, 0, 40, 14);
        let inner = Rect::new(3, 2, 20, 9);
        let mut buf = Buffer::empty(outer);
        HeatMap::new(&[(MONDAY, 2)], MONDAY + 3, styles()).render(inner, &mut buf);
        for y in 0..outer.height {
            for x in 0..outer.width {
                if !inner.contains((x, y).into()) {
                    assert_eq!(sym(&buf, x, y), " ", "({x},{y})");
                }
            }
        }
    }

    #[test]
    fn tiny_areas_do_not_panic() {
        for w in 0..8 {
            for h in 0..4 {
                let _ = render(&[(MONDAY, 1)], MONDAY, w, h);
            }
        }
    }

    #[test]
    fn legend_reads_less_to_more() {
        let text = |density| -> String {
            legend(styles(), density, Style::default())
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect()
        };
        assert_eq!(text(false), "less ■ ■ ■ ■ ■ more");
        assert_eq!(text(true), "less · ░ ▒ ▓ █ more");
    }
}
