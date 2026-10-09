//! Pure text helpers of the dashboard: relative times, dates, compact numbers,
//! and the figure next to a share bar. No drawing.

use ratatui::style::Style;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

use crate::components::ui::share_bar::{pad_label, percent_label};
use crate::git::stats::Window;
use crate::git::stats::series::{Granularity, civil_from_days};
use crate::git::stats::share::Share;

const DAY: i64 = 86_400;
/// A share of fewer items than this misleads: the screen shows counts.
pub(super) const MIN_WHOLE: u64 = 20;

/// The one relative-time formatter of the screen: "just now", "5 min ago",
/// "2 h ago", "3 d ago", "6 w ago", "4 mo ago", "2 y ago". Never "0 h ago".
pub(super) fn relative_time(now: i64, then: i64) -> String {
    let secs = (now - then).max(0);
    let days = secs / DAY;
    if secs < 60 {
        "just now".to_owned()
    } else if secs < 3_600 {
        format!("{} min ago", secs / 60)
    } else if secs < DAY {
        format!("{} h ago", secs / 3_600)
    } else if days < 14 {
        format!("{days} d ago")
    } else if days < 60 {
        format!("{} w ago", days / 7)
    } else if days < 365 {
        format!("{} mo ago", days / 30)
    } else {
        format!("{} y ago", days / 365)
    }
}

pub(super) const fn window_label(window: Window) -> &'static str {
    match window {
        Window::Days7 => "7 days",
        Window::Days30 => "30 days",
        Window::Days90 => "90 days",
        Window::Year => "1 year",
        Window::All => "all time",
    }
}

/// `YYYY-MM-DD` (UTC).
pub(super) fn date(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(DAY));
    format!("{y:04}-{m:02}-{d:02}")
}

/// An axis label: `YYYY-MM` for months, else `MM-DD`, or the whole date when
/// the axis spans more than a year.
pub(super) fn axis_date(secs: i64, granularity: Granularity, long: bool) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(DAY));
    match (granularity, long) {
        (Granularity::Month, _) => format!("{y:04}-{m:02}"),
        (_, true) => format!("{y:04}-{m:02}-{d:02}"),
        _ => format!("{m:02}-{d:02}"),
    }
}

/// `1 commit`, `3 commits`.
pub(super) fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `999`, `87.5k`, `1.2M`.
pub(super) fn compact(n: u64) -> String {
    let tenths = |n: u64, unit: u64| {
        let t = n * 10 / unit;
        format!("{}.{}", t / 10, t % 10)
    };
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{}k", tenths(n, 1_000))
    } else {
        format!("{}M", tenths(n, 1_000_000))
    }
}

/// `20 000`: groups of three, a space between.
pub(super) fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// The figure beside a bar: `41 %  (124)`, or `124  (41 %)` when counts are
/// asked for, and only the count when the whole has fewer than `MIN_WHOLE`
/// items. `–` when there is no whole.
pub(super) fn figure(share: Share, whole: u64, show_counts: bool) -> String {
    if whole == 0 {
        return "–".to_owned();
    }
    if whole < MIN_WHOLE {
        return share.count.to_string();
    }
    let percent = if share.under_one { None } else { share.percent };
    percent_label(percent, share.count, show_counts)
}

/// The figures of one chart lined up in two columns: the first figure right
/// aligned, the bracketed second one left aligned and dimmed. Also the total
/// width of a figure.
pub(super) fn figure_columns(figures: &[String], dim: Style) -> (Vec<Vec<Span<'static>>>, usize) {
    let split: Vec<(&str, String)> = figures
        .iter()
        .map(|f| match f.split_once("  (") {
            Some((first, rest)) => (first, format!("  ({rest}")),
            None => (f.as_str(), String::new()),
        })
        .collect();
    let first_w = split
        .iter()
        .map(|(f, _)| UnicodeWidthStr::width(*f))
        .max()
        .unwrap_or(0);
    let rest_w = split
        .iter()
        .map(|(_, r)| UnicodeWidthStr::width(r.as_str()))
        .max()
        .unwrap_or(0);
    let spans = split
        .iter()
        .map(|(first, rest)| {
            let mut spans = vec![Span::raw(pad_label(first, first_w))];
            if rest_w > 0 {
                spans.push(Span::styled(format!("{rest:<rest_w$}"), dim));
            }
            spans
        })
        .collect();
    (spans, first_w + rest_w)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "a failed setup or a bad index is the assertion in a test"
)]
mod tests {
    use super::*;

    #[test]
    fn relative_times() {
        let now = 1_000_000_000;
        for (ago, want) in [
            (0, "just now"),
            (59, "just now"),
            (60, "1 min ago"),
            (300, "5 min ago"),
            (3_599, "59 min ago"),
            (3_600, "1 h ago"),
            (2 * 3_600, "2 h ago"),
            (86_399, "23 h ago"),
            (DAY, "1 d ago"),
            (3 * DAY, "3 d ago"),
            (13 * DAY, "13 d ago"),
            (14 * DAY, "2 w ago"),
            (42 * DAY, "6 w ago"),
            (60 * DAY, "2 mo ago"),
            (120 * DAY, "4 mo ago"),
            (364 * DAY, "12 mo ago"),
            (365 * DAY, "1 y ago"),
            (800 * DAY, "2 y ago"),
        ] {
            assert_eq!(relative_time(now, now - ago), want, "{ago}");
        }
        assert_eq!(relative_time(now, now + 500), "just now", "a clock skew");
    }

    #[test]
    fn never_zero_hours() {
        for secs in (0..DAY).step_by(97) {
            assert!(!relative_time(1_000_000, 1_000_000 - secs).starts_with("0 "));
        }
    }

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_790_683_200), "2026-09-29");
        assert_eq!(axis_date(1_790_683_200, Granularity::Day, false), "09-29");
        assert_eq!(
            axis_date(1_790_683_200, Granularity::Week, true),
            "2026-09-29"
        );
        assert_eq!(
            axis_date(1_790_683_200, Granularity::Month, false),
            "2026-09"
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1_000), "1.0k");
        assert_eq!(compact(87_500), "87.5k");
        assert_eq!(compact(16_149), "16.1k");
        assert_eq!(compact(1_250_000), "1.2M");
        assert_eq!(thousands(5), "5");
        assert_eq!(thousands(5_000), "5 000");
        assert_eq!(thousands(20_000), "20 000");
        assert_eq!(thousands(1_234_567), "1 234 567");
        assert_eq!(plural(1, "commit", "commits"), "1 commit");
        assert_eq!(plural(0, "commit", "commits"), "0 commits");
    }

    #[test]
    fn figures_follow_the_rules() {
        let share = Share::of(29, 100);
        assert_eq!(figure(share, 100, false), "29 %  (29)");
        assert_eq!(figure(share, 100, true), "29  (29 %)");
        assert_eq!(
            figure(Share::of(3, 10), 10, false),
            "3",
            "a whole under 20 shows counts"
        );
        assert_eq!(figure(Share::of(0, 0), 0, false), "–");
        assert_eq!(figure(Share::of(1, 1000), 1000, false), "<1 %  (1)");
    }

    #[test]
    fn figure_columns_line_up_and_dim_the_bracketed_part() {
        let dim = Style::new().add_modifier(ratatui::style::Modifier::DIM);
        let figures = ["29 %  (124)".to_owned(), "5 %  (20)".to_owned()];
        let (spans, width) = figure_columns(&figures, dim);
        assert_eq!(width, 4 + 7);
        assert_eq!(spans[0][0].content, "29 %");
        assert_eq!(spans[1][0].content, " 5 %");
        assert_eq!(spans[1][1].content, "  (20) ");
        assert_eq!(spans[0][1].style, dim);
        let (plain, width) = figure_columns(&["7".to_owned(), "12".to_owned()], dim);
        assert_eq!((plain[0].len(), width), (1, 2));
    }
}
