//! The git identities the author label and Ferrit's commits use.

pub mod settings;

use self::settings::Settings;

#[derive(Debug, Clone, Default)]
pub struct Profile {
    pub settings: Settings,
}

impl Profile {
    pub const fn new(settings: Settings) -> Self {
        Self { settings }
    }
}
