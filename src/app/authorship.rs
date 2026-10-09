//! Who ferrit's commits are by. Git knows its own identity and the ones in the
//! global config (`Profile`); ferrit lets the user pick another for this run
//! (`Ctrl-A` in the commit popup) without writing git's config. Every commit
//! ferrit makes (the popup, a fixup, the first commit of a new repository)
//! reads the pick from here.

use crate::git::port::{GitPort, GitRead};
use crate::git::profile::Profile;
use crate::git::profile::settings::{Identity, IdentitySource, Settings};

pub(crate) struct Authorship {
    /// The identities git knows, refreshed with the repository.
    pub(super) profile: Profile,
    /// Ferrit's pick for this run; `None` is git's own.
    pub(super) selected: Option<Identity>,
    /// `user.name` as git resolves it here, shown in the info panel.
    pub(super) git_user_name: Option<String>,
}

impl Authorship {
    /// The authorship of a session over `repo`, or of the repo-free one.
    pub(super) fn of(repo: Option<&dyn GitPort>) -> Self {
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

    /// The identities git knows for `repo`.
    pub(super) fn profile_of(repo: &dyn GitPort) -> Profile {
        let (global_identities, repository_identity, effective_identity, identity_source) =
            repo.identity_settings();
        Profile::new(Settings {
            global_identities,
            repository_identity,
            effective_identity,
            identity_source,
        })
    }

    pub(super) fn new(profile: Profile, git_user_name: Option<String>) -> Self {
        Self {
            profile,
            selected: None,
            git_user_name,
        }
    }

    /// The `--author` value for a commit: the pick as `Name <email>`, or `None`
    /// to let git use its own. A pick without an email is not a usable author.
    pub(super) fn author_arg(&self) -> Option<String> {
        let identity = self.selected.as_ref()?;
        let email = identity.email.as_ref()?;
        Some(format!("{} <{email}>", identity.name))
    }

    /// The popup's author line: who the next commit is by.
    pub(super) fn line(&self) -> String {
        let show = |i: &Identity| match &i.email {
            Some(email) => format!("{} <{email}>", i.name),
            None => i.name.clone(),
        };
        match (&self.selected, &self.profile.settings.effective_identity) {
            (Some(chosen), _) => format!("author: {}", show(chosen)),
            (None, Some(own)) => format!("author: git's own ({})", show(own)),
            (None, None) => "author: git's own".to_owned(),
        }
    }

    /// `Ctrl-A`: the next identity git knows (from its global config), then
    /// git's own, then round again. Nothing to cycle when git knows none.
    pub(super) fn cycle(&mut self) {
        let identities = self.profile.settings.available_identities();
        self.selected = match &self.selected {
            None => identities.first().cloned(),
            Some(current) => identities
                .iter()
                .position(|i| i == current)
                .and_then(|index| identities.get(index + 1))
                .cloned(),
        };
    }
}
