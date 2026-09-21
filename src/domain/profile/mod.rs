//! Profile domain model assembled from settings and repository activity.

mod activity;
mod settings;

pub use activity::{Activity, ActivityWeek};
pub use settings::{Identity, Settings};

#[derive(Debug, Clone, Default)]
pub struct Profile {
    pub settings: Settings,
    pub activity: Activity,
}

impl Profile {
    pub fn new(settings: Settings, commit_timestamps: &[i64], push_timestamps: &[i64]) -> Self {
        Self {
            settings,
            activity: Activity::from_timestamps(commit_timestamps, push_timestamps),
        }
    }
}
