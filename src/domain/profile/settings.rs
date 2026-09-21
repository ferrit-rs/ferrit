//! Profile settings model, independent of Git config and terminal UI.

#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub global_identity: Option<Identity>,
    pub repository_identity: Option<Identity>,
    pub effective_identities: Vec<Identity>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub email: Option<String>,
}
