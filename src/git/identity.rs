//! Profile settings model, independent of Git config and terminal UI.

use crate::git::port::{GitPort, GitRead};

/// Who git thinks the user is, and where that comes from.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// The identities in the user's global git config (name and email pairs).
    pub global_identities: Vec<Identity>,
    /// The identity the repository's own config sets, if it sets one.
    pub repository_identity: Option<Identity>,
    /// The identity git would commit with here.
    pub effective_identity: Option<Identity>,
    /// Which config level `effective_identity` comes from.
    pub identity_source: IdentitySource,
}

impl Settings {
    /// The global identities that have an email, which is what a commit author needs.
    pub fn available_identities(&self) -> Vec<Identity> {
        self.global_identities
            .iter()
            .filter(|identity| identity.email.is_some())
            .cloned()
            .collect()
    }
}

/// Which config level an identity comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IdentitySource {
    /// The repository's own config.
    Repository,
    /// The user's global config.
    Global,
    /// The system config.
    System,
    /// No identity is configured.
    #[default]
    Unset,
}

/// A name and an email, as in `Name <email>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    /// The name (`user.name`).
    pub name: String,
    /// The email (`user.email`), if there is one.
    pub email: Option<String>,
}

/// The identities git knows for this repository.
#[derive(Debug, Clone, Default)]
pub struct Profile {
    /// The identities and where they come from.
    pub settings: Settings,
}

impl Profile {
    /// A profile holding `settings`.
    pub const fn new(settings: Settings) -> Self {
        Self { settings }
    }
}

pub(crate) struct Authorship {
    /// The identities git knows, refreshed with the repository.
    pub(crate) profile: Profile,
    /// Ferrit's pick for this run; `None` is git's own.
    pub(crate) selected: Option<Identity>,
    /// `user.name` as git resolves it here, shown in the info panel.
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

    /// The identities git knows for `repo`.
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

    /// The `--author` value for a commit: the pick as `Name <email>`, or `None`
    /// to let git use its own. A pick without an email is not a usable author.
    pub(crate) fn author_arg(&self) -> Option<String> {
        let identity = self.selected.as_ref()?;
        let email = identity.email.as_ref()?;
        Some(format!("{} <{email}>", identity.name))
    }

    /// The popup's author line: who the next commit is by.
    pub(crate) fn line(&self) -> String {
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
    pub(crate) fn cycle(&mut self) {
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

#[cfg(test)]
mod tests_identity {
    use crate::git::identity::{Identity, IdentitySource, Settings};

    #[test]
    fn available_identities_uses_global_config_only_and_requires_email() {
        let global = Identity {
            name: "Global User".to_owned(),
            email: Some("global@example.com".to_owned()),
        };
        let no_email = Identity {
            name: "Incomplete User".to_owned(),
            email: None,
        };
        let repository = Identity {
            name: "Repository User".to_owned(),
            email: Some("repo@example.com".to_owned()),
        };
        let settings = Settings {
            global_identities: vec![global.clone(), no_email],
            repository_identity: Some(repository.clone()),
            effective_identity: Some(repository),
            identity_source: IdentitySource::Repository,
        };

        assert_eq!(settings.available_identities(), vec![global]);
    }
}
