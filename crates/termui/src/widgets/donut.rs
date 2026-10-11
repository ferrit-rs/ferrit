//! A donut chart on a ratatui `Canvas`. See `docs/PLAN_13_DASHBOARD.md`, "Charts".
//!
//! The ring is sampled on a square grid of Braille dots (2 x 4 dots per cell, so a
//! cell that is about twice as tall as wide still gives a round ring). Angles come
//! from the slice weights in the given order, from 12 o'clock, clockwise. The
//! caller folds small slices first; nothing is folded here.

use std::f64::consts::TAU;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::symbols::Marker;
use ratatui::widgets::Widget;
use ratatui::widgets::canvas::{Canvas, Painter, Shape};

/// Smallest area (columns) that draws a ring; below it use a share bar.
pub const MIN_WIDTH: u16 = 12;
/// Smallest area (rows) that draws a ring; below it use a share bar.
pub const MIN_HEIGHT: u16 = 6;

/// Inner radius as a share of the outer one: the ring is a quarter of the radius thick.
const INNER: f64 = 0.75;
/// Half the gap between two slices, in dots along the arc: dots within it of a
/// boundary are left blank, so the gap is about one dot wide (two when the boundary
/// falls between two dot columns, as at 12 o'clock).
const HALF_GAP: f64 = 0.6;

/// One slice of the donut: its weight and its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slice {
    /// Share of the whole, in any unit; only the ratio to the other weights counts.
    pub weight: u64,
    /// Colour of every cell that this slice covers the most.
    pub color: Color,
}

/// The donut widget. Draws nothing when there is no weight or the area is too small.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct Donut<'a> {
    slices: &'a [Slice],
}

impl<'a> Donut<'a> {
    pub fn new(slices: &'a [Slice]) -> Self {
        Self { slices }
    }
}

/// True when `area` is big enough for a ring (`MIN_WIDTH` x `MIN_HEIGHT`).
pub fn fits(area: Rect) -> bool {
    area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT
}

/// Index of the slice under `angle` (degrees, from 12 o'clock, clockwise; any value
/// wraps into one turn). A boundary belongs to the next slice. `None` when the
/// total weight is zero.
#[allow(clippy::cast_precision_loss)] // weights are counts, far below 2^53
pub fn slice_at(angle: f64, slices: &[Slice]) -> Option<usize> {
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

#[cfg(test)]
mod tests {
    use crate::widgets::donut::{Donut, MIN_HEIGHT, MIN_WIDTH, Slice, fits, slice_at};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::widgets::Widget;

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
