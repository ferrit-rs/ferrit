//! Creating the GitHub repository from ferrit (`docs/PLAN_15_CREATE_REMOTE.md`,
//! R2, R3): the draft the user typed, the popups that fill it (the `gh`
//! check, the form, the last question), the background `gh repo create`, and
//! what happens when it answers.
//!
//! The creation takes the same slot as fetch, pull and push (`remote_busy`),
//! so one network operation runs at a time, the Status pane shows it, and quit
//! cancels it the way it cancels them.

use std::path::PathBuf;

use crate::git::host::{CreateDraft, GhProgram};
use crate::git::ssh_config::read_github_aliases;

/// The creation's own state on `App`.
#[derive(Debug, Default)]
pub struct CreateRemote {
    /// The last draft, until a creation succeeds.
    pub draft: Option<CreateDraft>,
    /// Why the last creation was refused, for the form to show.
    pub error: Option<String>,
    /// The web URL of the repository just created.
    pub web_url: Option<String>,
    pub(crate) gh: GhProgram,
    /// Bumped each time the check starts or is abandoned.
    pub(crate) generation: u64,
    /// The push in flight is the one that follows a creation.
    pub(crate) pushing_after: bool,
    /// The ssh config the host aliases are read from; `None` is
    /// `~/.ssh/config`. A test points it elsewhere.
    pub(crate) ssh_config: Option<PathBuf>,
    /// The SSH host chosen when the creation started (the user's own alias, or
    /// none), used once `gh` has made the repository.
    pub(crate) ssh_host: String,
}

impl CreateRemote {
    /// Keep the `gh` program a test or the replay injected when the app is
    /// rebuilt on a new repository (`App::attach_repository`).
    pub(crate) fn carry_program_from(&mut self, previous: &Self) {
        self.gh = previous.gh.clone();
        self.ssh_config.clone_from(&previous.ssh_config);
    }

    /// The GitHub aliases of the ssh config, in the order it lists them.
    pub(crate) fn ssh_aliases(&self) -> Vec<String> {
        let path = self.ssh_config.clone().or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".ssh/config"))
        });
        path.map_or_else(Vec::new, |path| read_github_aliases(&path))
    }
}
