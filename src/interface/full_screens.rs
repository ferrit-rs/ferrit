//! The views that replace the five panes: the git config editor and the
//! welcome screen for a folder with no repository (`docs/PLAN_13_DASHBOARD.md`,
//! `docs/PLAN_14_GIT_CONFIG.md`, `docs/PLAN_16_START_WITHOUT_REPO.md`). The
//! keys and drawing of each are `git_config` and `welcome`; this is their state.

use std::path::PathBuf;

use crate::git::keys::git_config;

#[derive(Default)]
pub(crate) struct FullScreens {
    /// The view showing, if any.
    pub(crate) active: FullScreen,
    pub(crate) git_config: git_config::GitConfigScreen,
    /// The folder the welcome screen is about; `None` once there is a repository.
    pub(crate) welcome_dir: Option<PathBuf>,
    /// The highlighted row of the welcome screen: 0 is `git init`, 1 is quit.
    pub(crate) welcome_selected: usize,
}

/// A view that takes the whole terminal in place of the five panes: the git
/// config editor (`docs/PLAN_14_GIT_CONFIG.md`) and the welcome screen. (The
/// dashboard was one until phase 19: it is a sheet now, `app::sheet`.)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FullScreen {
    #[default]
    None,
    GitConfig,
    /// No repository: ferrit started in a folder that is not one
    /// (`docs/PLAN_16_START_WITHOUT_REPO.md`).
    Welcome,
}
