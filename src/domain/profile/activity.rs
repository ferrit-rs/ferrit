//! Convert repository commits into a year of daily activity and a recent feed.

use crate::domain::git::model::CommitEntry;

const DAYS_PER_WEEK: usize = 7;
const WEEKS_PER_YEAR: usize = 52;
const DAY_SECONDS: i64 = 86_400;

#[derive(Debug, Clone, Default)]
pub struct Activity {
    pub weeks: Vec<ActivityWeek>,
    pub commit_count: u32,
    pub recent_commits: Vec<CommitEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct ActivityWeek {
    /// Monday through Sunday commit counts.
    pub days: [u32; DAYS_PER_WEEK],
    pub month_label: Option<&'static str>,
}

impl Activity {
    pub fn from_commits(commits: &[CommitEntry]) -> Self {
        let today = today_days();
        let weekday = (today + 3).rem_euclid(7); // Unix epoch began Thursday.
        let start = today
            .saturating_sub(weekday)
            .saturating_sub(i64::try_from(WEEKS_PER_YEAR - 1).unwrap_or(0) * 7);
        let mut weeks = Vec::with_capacity(WEEKS_PER_YEAR);
        let mut previous_month = 0;
        for week_index in 0..WEEKS_PER_YEAR {
            let week_offset = i64::try_from(week_index).unwrap_or(0);
            let week_start = start.saturating_add(week_offset * 7);
            let (_, month, _) = civil_date(week_start + 3);
            let week = ActivityWeek {
                days: [0; DAYS_PER_WEEK],
                month_label: if month != previous_month {
                    previous_month = month;
                    Some(month_name(month))
                } else {
                    None
                },
            };
            weeks.push(week);
        }
        for commit in commits {
            let timestamp = commit.time;
            let offset = timestamp.div_euclid(DAY_SECONDS).saturating_sub(start);
            if let Ok(offset) = usize::try_from(offset) {
                if let Some(week) = weeks.get_mut(offset / DAYS_PER_WEEK)
                    && let Some(day_count) = week.days.get_mut(offset % DAYS_PER_WEEK)
                {
                    *day_count = day_count.saturating_add(1);
                }
            }
        }
        let commit_count = weeks
            .iter()
            .flat_map(|week| week.days)
            .fold(0_u32, u32::saturating_add);
        let range_end = start + i64::try_from(WEEKS_PER_YEAR * DAYS_PER_WEEK).unwrap_or(0);
        let recent_commits = commits
            .iter()
            .filter(|commit| {
                let day = commit.time.div_euclid(DAY_SECONDS);
                day >= start && day < range_end
            })
            .take(12)
            .cloned()
            .collect();
        Self {
            weeks,
            commit_count,
            recent_commits,
        }
    }
}

fn today_days() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0_i64, |duration| {
            i64::try_from(duration.as_secs() / 86_400_u64).unwrap_or(i64::MAX)
        })
}

fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

fn month_name(month: u32) -> &'static str {
    [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .get(usize::try_from(month.saturating_sub(1)).unwrap_or(12))
    .copied()
    .unwrap_or(" ")
}
