//! A 100 % share bar. See `docs/PLAN_13_DASHBOARD.md`, "Charts".
//!
//! `LineGauge` draws a thin line glyph with its own label and one ratio, so it
//! cannot be a stacked bar or a span run inside a table row; these helpers draw
//! `█░` spans instead. Colours always come from the caller (the theme).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

const FULL: &str = "█";
const EMPTY: &str = "░";

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

/// One share as `█` (in `fg`) followed by dim `░`, exactly `width` cells.
pub fn single_bar(fraction: f64, width: u16, fg: Color) -> Vec<Span<'static>> {
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
/// cells. A zero total gives a dim `░` line.
pub fn stacked_bar(parts: &[(u64, Color)], width: u16) -> Line<'static> {
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
pub fn percent_label(percent: Option<u8>, count: u64, show_counts: bool) -> String {
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
pub fn pad_label(label: &str, width: usize) -> String {
    format!("{label:>width$}")
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "test scaffolding: an out-of-range index is the failed assertion"
)]
mod tests {
    use super::*;

    fn text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn filled(spans: &[Span<'_>]) -> usize {
        text(spans).chars().filter(|&c| c == '█').count()
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
        assert_eq!(bar[0].content, "██");
        assert_eq!(bar[1].style.fg, None);
        assert!(bar[1].style.add_modifier.contains(Modifier::DIM));
        assert_eq!(bar[1].content, "░░");
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
            assert_eq!(line.spans[0].content, "░░░░░░");
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
