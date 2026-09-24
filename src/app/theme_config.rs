//! User-selected accent theme, persisted in the platform config directory.

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
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
pub(super) enum ThemeMode {
    #[default]
    Idle,
    Palette,
    EditingRgb,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Preset {
    #[default]
    Green,
    Blue,
    Purple,
    Amber,
}

impl Preset {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Amber => "Amber",
        }
    }

    pub(super) const fn color(self) -> Color {
        match self {
            Self::Green => Color::Green,
            Self::Blue => Color::Cyan,
            Self::Purple => Color::Magenta,
            Self::Amber => Color::Yellow,
        }
    }

    pub(super) const fn next(self) -> Self {
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
pub(super) struct ThemeConfig {
    pub(super) preset: Preset,
    /// Optional RGB override; TOML uses Ratatui's `#RRGGBB` serde format.
    pub(super) accent: Option<Color>,
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
    pub(super) fn color(&self) -> Color {
        self.accent.unwrap_or_else(|| self.preset.color())
    }

    fn path() -> Option<PathBuf> {
        ProjectDirs::from("dev", "Ferrit", "Ferrit")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    pub(super) fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| toml::from_str::<ConfigFile>(&raw).ok())
            .map(|file| file.theme)
            .unwrap_or_default()
    }

    pub(super) fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(&ConfigFile {
            theme: self.clone(),
        })
        .map_err(std::io::Error::other)?;
        fs::write(path, body)
    }
}

#[derive(Deserialize, Serialize)]
struct ConfigFile {
    theme: ThemeConfig,
}
