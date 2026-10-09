//! The views that replace the five panes: the git config editor and the
//! welcome screen for a folder with no repository (`docs/PLAN_13_DASHBOARD.md`,
//! `docs/PLAN_14_GIT_CONFIG.md`, `docs/PLAN_16_START_WITHOUT_REPO.md`). The
//! keys and drawing of each are `git_config` and `welcome`; this is their state.

use std::path::PathBuf;

use super::{FullScreen, git_config};

#[derive(Default)]
pub(super) struct FullScreens {
    /// The view showing, if any.
    pub(super) active: FullScreen,
    pub(super) git_config: git_config::GitConfigScreen,
    /// The folder the welcome screen is about; `None` once there is a repository.
    pub(super) welcome_dir: Option<PathBuf>,
    /// The highlighted row of the welcome screen: 0 is `git init`, 1 is quit.
    pub(super) welcome_selected: usize,
}
