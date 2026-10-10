//! Session authorship selection for the commit editor.

use ferrit_domain::identity::{Identity, IdentitySource, Profile, Settings};
use ferrit_domain::port::{GitPort, GitRead};

/// The authorship choices attached to one application session.
pub(crate) struct Authorship {
    /// The identities Git knows, refreshed with the repository.
    pub(crate) profile: Profile,
    /// Ferrit's pick for this run. `None` means Git's own identity.
    selected: Option<Identity>,
    /// `user.name` as Git resolves it here, shown in the info panel.
    pub(crate) git_user_name: Option<String>,
}

impl Authorship {
    /// The authorship of a session over `repo`, or of the repo-free one.
    pub(crate) fn of(repo: Option<&dyn GitPort>) -> Self {
        let profile = repo.map_or_else(
            || {
                Profile::new(Settings {
                    global_identities: Vec::new(),
                    repository_identity: None,
                    effective_identity: None,
                    identity_source: IdentitySource::Unset,
                })
            },
            Self::profile_of,
        );
        Self::new(profile, repo.and_then(GitRead::user_name))
    }

    /// The identities Git knows for `repo`.
    pub(crate) fn profile_of(repo: &dyn GitPort) -> Profile {
        let (global_identities, repository_identity, effective_identity, identity_source) =
            repo.identity_settings();
        Profile::new(Settings {
            global_identities,
            repository_identity,
            effective_identity,
            identity_source,
        })
    }

    pub(crate) fn new(profile: Profile, git_user_name: Option<String>) -> Self {
        Self {
            profile,
            selected: None,
            git_user_name,
        }
    }

    /// The `--author` value for a commit, or `None` for Git's own identity.
    pub(crate) fn author_arg(&self) -> Option<String> {
        let identity = self.selected.as_ref()?;
        let email = identity.email.as_ref()?;
        Some(format!("{} <{email}>", identity.name))
    }

    /// The popup's author line.
    pub(crate) fn line(&self) -> String {
        let show = |identity: &Identity| match &identity.email {
            Some(email) => format!("{} <{email}>", identity.name),
            None => identity.name.clone(),
        };
        match (&self.selected, &self.profile.settings.effective_identity) {
            (Some(chosen), _) => format!("author: {}", show(chosen)),
            (None, Some(own)) => format!("author: git's own ({})", show(own)),
            (None, None) => "author: git's own".to_owned(),
        }
    }

    /// `Ctrl-A`: cycle through the usable global identities, then Git's own.
    pub(crate) fn cycle(&mut self) {
        let identities = self.profile.settings.available_identities();
        self.selected = match &self.selected {
            None => identities.first().cloned(),
            Some(current) => identities
                .iter()
                .position(|identity| identity == current)
                .and_then(|index| identities.get(index + 1))
                .cloned(),
        };
    }
}
