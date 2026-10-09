//! The git identities the author label and Ferrit's commits use.

pub mod settings;

use self::settings::Settings;

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
