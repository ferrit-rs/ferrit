//! Profile settings model, independent of Git config and terminal UI.

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

#[cfg(test)]
mod tests_identity {
    use crate::identity::{Identity, IdentitySource, Settings};

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
