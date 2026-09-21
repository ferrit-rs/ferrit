//! Profile settings and repository-wide commit activity.

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
    pub fn new(settings: Settings, commits: &[crate::domain::git::model::CommitEntry]) -> Self {
        Self {
            settings,
            activity: Activity::from_commits(commits),
        }
    }
}
