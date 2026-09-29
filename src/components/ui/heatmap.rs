//! A commits-per-day heat map. See `docs/PLAN_13_DASHBOARD.md`, "Charts".
//!
//! Weeks run in columns, days in rows (Monday first), two cells per day. Days are
//! unix day numbers (days since 1970-01-01), so no calendar library is needed. The
//! widget owns no colour: the caller passes one [`Style`] per level.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

/// The glyph of each level: none, then four buckets of non-zero days.
const GLYPHS: [&str; 5] = ["·", "░", "▒", "▓", "█"];
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
pub fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

/// The three quartile cut-offs (nearest rank) of the non-zero `counts`.
///
/// Zeros are ignored. With no non-zero count every cut-off is 0.
pub fn thresholds(counts: &[u32]) -> [u32; 3] {
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
pub fn level(count: u32, thresholds: [u32; 3]) -> usize {
    if count == 0 {
        return 0;
    }
    1 + thresholds.iter().filter(|&&t| count > t).count()
}

/// The `░ few  █ many  · none` legend line, in the same level styles as the map.
pub fn legend(styles: [Style; 5]) -> Line<'static> {
    let [none, few, _, _, many] = styles;
    Line::from(vec![
        Span::styled(GLYPHS[1], few),
        Span::raw(" few  "),
        Span::styled(GLYPHS[4], many),
        Span::raw(" many  "),
        Span::styled(GLYPHS[0], none),
        Span::raw(" none"),
    ])
}

/// A commits-per-day heat map.
///
/// Levels come from the quartiles of the non-zero days shown, so one burst day does
/// not flatten the others. Days after `today` stay blank. The header labels every
/// fourth column `W1`, `W5`, ... counting weeks from the oldest one shown.
#[derive(Debug, Clone)]
pub struct HeatMap<'a> {
    counts: &'a [(i64, u32)],
    today: i64,
    styles: [Style; 5],
}

impl<'a> HeatMap<'a> {
    /// `counts` are `(unix day, commits)`, in any order; a repeated day is summed.
    /// `styles[0]` styles the `·` of a quiet day, `styles[1..]` the four buckets.
    pub fn new(counts: &'a [(i64, u32)], today: i64, styles: [Style; 5]) -> Self {
        Self {
            counts,
            today,
            styles,
        }
    }
}

impl Widget for HeatMap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let weeks = (area.width.saturating_sub(GUTTER) / CELL).min(MAX_WEEKS);
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
                        put(
                            x.saturating_add(dx),
                            area.y,
                            &ch.to_string(),
                            Style::default(),
                        );
                    }
                }
            }
        }
        for (&(wd, name), y) in rows.iter().zip(area.y.saturating_add(1)..area.bottom()) {
            for (dx, ch) in (0..).zip(name.chars()) {
                put(
                    area.x.saturating_add(dx),
                    y,
                    &ch.to_string(),
                    Style::default(),
                );
            }
            for col in 0..weeks {
                let day = first_monday + 7 * i64::from(col) + wd;
                if day > self.today {
                    continue;
                }
                let lvl = level(per_day.get(&day).copied().unwrap_or(0), cuts);
                let style = self.styles.get(lvl).copied().unwrap_or_default();
                let x = grid_x.saturating_add(col * CELL);
                put(x, y, GLYPHS.get(lvl).copied().unwrap_or("·"), style);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

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
        for (y, want) in [(1, "·"), (2, "·"), (3, "·"), (4, " "), (7, " ")] {
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
        let dots = row_text(&wide, 1).matches('·').count();
        assert_eq!(dots, 26);
        let narrow = render(&[], MONDAY + 6, 4 + 2 * 5 + 1, 9);
        assert_eq!(row_text(&narrow, 1).matches('·').count(), 5);
        assert_eq!(
            row_text(&render(&[], MONDAY, 5, 9), 1).matches('·').count(),
            0
        );
    }

    #[test]
    fn last_column_is_the_week_of_today() {
        // today = Friday: the last column shows Mon..Fri, with today's count.
        let today = MONDAY + 4;
        let buf = render(&[(today, 3)], today, 4 + 2 * 3, 9);
        let last = GUTTER + 2 * 2;
        assert_eq!(sym(&buf, last, 5), "░", "Friday holds today's commits");
        assert_eq!(sym(&buf, last, 6), " ", "Saturday is in the future");
        assert_eq!(sym(&buf, last, 1), "·");
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
        assert_eq!(sym(&buf, GUTTER, 4), "█");
    }

    #[test]
    fn empty_data_draws_the_quiet_grid_and_zero_area_nothing() {
        let buf = render(&[], MONDAY + 6, 20, 9);
        assert!(row_text(&buf, 3).contains('·'));
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
    fn legend_reads_few_many_none() {
        let text: String = legend(styles())
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(text, "░ few  █ many  · none");
    }
}
