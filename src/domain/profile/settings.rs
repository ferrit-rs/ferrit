//! Profile settings model, independent of Git config and terminal UI.

#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub global_identities: Vec<Identity>,
    pub repository_identity: Option<Identity>,
    pub effective_identity: Option<Identity>,
    pub identity_source: IdentitySource,
}

impl Settings {
    pub fn available_identities(&self) -> Vec<Identity> {
        let mut identities = self.global_identities.clone();
        if let Some(repository_identity) = &self.repository_identity
            && !identities.contains(repository_identity)
        {
            identities.push(repository_identity.clone());
        }
        identities
            .into_iter()
            .filter(|identity| identity.email.is_some())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Identity, IdentitySource, Settings};

    #[test]
    fn available_identities_deduplicates_and_requires_email() {
        let global = Identity {
            name: "Global User".to_owned(),
            email: Some("global@example.com".to_owned()),
        };
        let no_email = Identity {
            name: "Incomplete User".to_owned(),
            email: None,
        };
        let settings = Settings {
            global_identities: vec![global.clone(), no_email],
            repository_identity: Some(global.clone()),
            effective_identity: Some(global.clone()),
            identity_source: IdentitySource::Global,
        };

        assert_eq!(settings.available_identities(), vec![global]);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IdentitySource {
    Repository,
    Global,
    System,
    #[default]
    Unset,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub email: Option<String>,
}
