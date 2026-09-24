//! User-selected accent theme: the `[theme]` section of `config.toml`
//! (`app::config`) and the state of the drawer that edits it.

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

pub(super) const RGB_RED_CHANNEL: usize = 0;
pub(super) const RGB_GREEN_CHANNEL: usize = 1;
pub(super) const RGB_BLUE_CHANNEL: usize = 2;
pub(super) const RGB_CHANNEL_COUNT: usize = RGB_BLUE_CHANNEL + 1;

/// What the profile drawer's theme section is doing. The swatch picker and
/// the RGB editor never run together, so one mode replaces two flags that
/// could otherwise disagree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    Idle,
    Palette,
    EditingRgb,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    #[default]
    Green,
    Blue,
    Purple,
    Amber,
}

impl Preset {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Amber => "Amber",
        }
    }

    pub const fn color(self) -> Color {
        match self {
            Self::Green => Color::Green,
            Self::Blue => Color::Cyan,
            Self::Purple => Color::Magenta,
            Self::Amber => Color::Yellow,
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Green => Self::Blue,
            Self::Blue => Self::Purple,
            Self::Purple => Self::Amber,
            Self::Amber => Self::Green,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ThemeConfig {
    pub preset: Preset,
    /// Optional RGB override; TOML uses Ratatui's `#RRGGBB` serde format.
    pub accent: Option<Color>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            preset: Preset::Green,
            accent: None,
        }
    }
}

impl ThemeConfig {
    pub fn color(&self) -> Color {
        self.accent.unwrap_or_else(|| self.preset.color())
    }
}
