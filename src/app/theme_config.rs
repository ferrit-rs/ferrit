//! User-selected accent theme: the `[theme]` section of `config.toml`
//! (`app::config`) and the state of the drawer that edits it.

use std::collections::BTreeMap;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::components::ui::palette::Palette;

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

/// `[theme] base`: the terminal the palette is meant for.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Base {
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ThemeConfig {
    /// `"dark"` (default) or `"light"`.
    pub base: Base,
    pub preset: Preset,
    /// Optional RGB override; TOML uses Ratatui's `#RRGGBB` serde format.
    pub accent: Option<Color>,
    /// `[theme.colors]`: any of the palette's colours by name, as `"#rrggbb"`
    /// or a colour name (`"red"`, `"light-blue"`), over the base's.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, Color>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            base: Base::Dark,
            preset: Preset::Green,
            colors: BTreeMap::new(),
            accent: None,
        }
    }
}

impl ThemeConfig {
    /// The colours ferrit draws with under this theme.
    pub fn palette(&self) -> Palette {
        let mut palette = match self.base {
            Base::Dark => Palette::DARK,
            Base::Light => Palette::LIGHT,
        };
        for (name, &color) in &self.colors {
            if let Some(slot) = palette.color_mut(name) {
                *slot = color;
            }
        }
        palette
    }

    /// Remove the `[theme.colors]` entries that name no colour, one message
    /// each; the others still apply.
    pub fn drop_unknown_colors(&mut self) -> Vec<String> {
        let mut probe = Palette::DARK;
        let unknown: Vec<String> = self
            .colors
            .keys()
            .filter(|name| probe.color_mut(name).is_none())
            .cloned()
            .collect();
        unknown
            .into_iter()
            .map(|name| {
                self.colors.remove(&name);
                format!(
                    "`theme.colors.{name}` is not a colour of the palette (one of {}), ignored",
                    Palette::NAMES
                )
            })
            .collect()
    }

    pub fn color(&self) -> Color {
        self.accent.unwrap_or_else(|| self.preset.color())
    }
}
