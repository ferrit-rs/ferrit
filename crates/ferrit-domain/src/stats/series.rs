//! Commits over time: day, ISO week or month buckets in UTC, so the same
//! repository draws the same chart on every machine. No git here.

use std::collections::BTreeMap;

const DAY: i64 = 86_400;

/// Resolution of the time series, chosen by the span it covers so a chart
/// never has four points across the whole width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    /// One bucket per day.
    Day,
    /// One bucket per ISO week, starting on Monday.
    Week,
    /// One bucket per calendar month.
    Month,
}

impl Granularity {
    /// Daily up to 60 days of history, weekly up to two years, monthly beyond.
    pub const fn for_span_days(days: i64) -> Self {
        if days <= 60 {
            Self::Day
        } else if days <= 730 {
            Self::Week
        } else {
            Self::Month
        }
    }
}

/// Commits in one bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bucket {
    /// First second (UTC) of the day, of the ISO week (Monday) or of the month.
    pub start: i64,
    /// Commits whose time falls in the bucket.
    pub commits: usize,
}

/// Days since 1970-01-01 of the civil date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil `(year, month, day)` of a day count since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// The day (since the epoch) a bucket containing `day` starts on.
fn bucket_day(day: i64, granularity: Granularity) -> i64 {
    match granularity {
        Granularity::Day => day,
        // 1970-01-01 was a Thursday, so `day + 3` counts from Monday.
        Granularity::Week => day - (day + 3).rem_euclid(7),
        Granularity::Month => {
            let (year, month, _) = civil_from_days(day);
            days_from_civil(year, month, 1)
        },
    }
}

fn next_bucket_day(start: i64, granularity: Granularity) -> i64 {
    match granularity {
        Granularity::Day => start + 1,
        Granularity::Week => start + 7,
        Granularity::Month => {
            let (year, month, _) = civil_from_days(start);
            if month == 12 {
                days_from_civil(year + 1, 1, 1)
            } else {
                days_from_civil(year, month + 1, 1)
            }
        },
    }
}

/// Every bucket from the one holding `first` to the one holding `last`
/// (unix seconds), empty ones included, oldest first. Commit times outside
/// the range are clamped into it (a commit dated in the future lands in the
/// last bucket).
pub fn series<I: IntoIterator<Item = i64>>(
    times: I,
    first: i64,
    last: i64,
    granularity: Granularity,
) -> Vec<Bucket> {
    let first_day = first.div_euclid(DAY);
    let last_day = last.div_euclid(DAY).max(first_day);
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for time in times {
        let day = time.div_euclid(DAY).clamp(first_day, last_day);
        *counts.entry(bucket_day(day, granularity)).or_default() += 1;
    }
    let end = bucket_day(last_day, granularity);
    let mut out = Vec::new();
    let mut day = bucket_day(first_day, granularity);
    while day <= end {
        out.push(Bucket {
            start: day * DAY,
            commits: counts.get(&day).copied().unwrap_or(0),
        });
        day = next_bucket_day(day, granularity);
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "a failed setup is the assertion in a test"
)]
mod tests {
    use crate::stats::series::{DAY, Granularity, civil_from_days, days_from_civil, series};

    fn day(y: i64, m: i64, d: i64) -> i64 {
        days_from_civil(y, m, d) * DAY
    }

    #[test]
    fn civil_dates_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        for n in [-800, -1, 0, 59, 60, 365, 11_016, 19_000, 20_000, 60_000] {
            let (y, m, d) = civil_from_days(n);
            assert_eq!(days_from_civil(y, m, d), n);
        }
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
    }

    #[test]
    fn granularity_follows_the_span() {
        assert_eq!(Granularity::for_span_days(26), Granularity::Day);
        assert_eq!(Granularity::for_span_days(60), Granularity::Day);
        assert_eq!(Granularity::for_span_days(61), Granularity::Week);
        assert_eq!(Granularity::for_span_days(200), Granularity::Week);
        assert_eq!(Granularity::for_span_days(730), Granularity::Week);
        assert_eq!(Granularity::for_span_days(5 * 365), Granularity::Month);
    }

    #[test]
    fn weeks_start_on_monday_and_follow_iso() {
        // 2026-09-29 is a Tuesday; its ISO week starts Monday 2026-09-28.
        let s = series(
            [day(2026, 9, 29), day(2026, 9, 21) + 5],
            day(2026, 9, 21),
            day(2026, 9, 29),
            Granularity::Week,
        );
        assert_eq!(s.len(), 2);
        assert_eq!(s.first().unwrap().start, day(2026, 9, 21));
        assert_eq!(s.last().unwrap().start, day(2026, 9, 28));
        assert_eq!(s.iter().map(|b| b.commits).collect::<Vec<_>>(), [1, 1]);
        // A Sunday belongs to the week that started six days before.
        let s = series(
            [day(2026, 9, 27)],
            day(2026, 9, 21),
            day(2026, 9, 28),
            Granularity::Week,
        );
        assert_eq!(s.iter().map(|b| b.commits).collect::<Vec<_>>(), [1, 0]);
    }

    #[test]
    fn days_fill_the_gaps_with_zero() {
        let s = series(
            [day(2026, 9, 1), day(2026, 9, 1) + 100, day(2026, 9, 4)],
            day(2026, 9, 1),
            day(2026, 9, 4),
            Granularity::Day,
        );
        assert_eq!(
            s.iter().map(|b| b.commits).collect::<Vec<_>>(),
            [2, 0, 0, 1]
        );
    }

    #[test]
    fn months_step_across_a_year_end() {
        let s = series(
            [day(2025, 12, 31), day(2026, 1, 1)],
            day(2025, 11, 15),
            day(2026, 2, 3),
            Granularity::Month,
        );
        assert_eq!(
            s.iter().map(|b| b.start).collect::<Vec<_>>(),
            [
                day(2025, 11, 1),
                day(2025, 12, 1),
                day(2026, 1, 1),
                day(2026, 2, 1)
            ]
        );
        assert_eq!(
            s.iter().map(|b| b.commits).collect::<Vec<_>>(),
            [0, 1, 1, 0]
        );
    }

    #[test]
    fn a_future_commit_lands_in_the_last_bucket() {
        let s = series(
            [day(2030, 1, 1)],
            day(2026, 9, 1),
            day(2026, 9, 3),
            Granularity::Day,
        );
        assert_eq!(s.iter().map(|b| b.commits).collect::<Vec<_>>(), [0, 0, 1]);
    }
}
