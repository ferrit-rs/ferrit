//! Profile settings and repository-wide commit activity.

pub mod activity;
pub mod settings;

use self::activity::Activity;
use self::settings::Settings;

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
