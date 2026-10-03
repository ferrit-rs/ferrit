//! User-selected accent theme: the `[theme]` section of `config.toml`
//! (`app::config`) and the state of the drawer that edits it.

use std::collections::BTreeMap;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::components::ui::palette::Palette;
use crate::components::ui::scheme::Scheme;

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

    /// Every preset, in the order `next` walks them.
    pub const ALL: [Self; 4] = [Self::Green, Self::Blue, Self::Purple, Self::Amber];

    #[must_use]
    pub const fn prev(self) -> Self {
        match self {
            Self::Green => Self::Amber,
            Self::Blue => Self::Green,
            Self::Purple => Self::Blue,
            Self::Amber => Self::Purple,
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

/// `[theme] scheme`: whether ferrit paints the screen itself
/// (`docs/PLAN_18_THEMES.md`).
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SchemeChoice {
    /// No background of its own: the terminal's colours, `base` saying whether
    /// the terminal is dark or light.
    Terminal,
    /// A dark screen, painted, whatever the terminal is.
    Dark,
    /// A light screen, painted, whatever the terminal is.
    Light,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ThemeConfig {
    /// `"terminal"`, `"dark"` or `"light"`. Absent, it is taken from `base`
    /// (`"dark"` or `"light"`, painted), so a file written before `terminal` came
    /// back keeps its look. Dark and Light make `base` irrelevant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<SchemeChoice>,
    /// `"dark"` (default) or `"light"`: the terminal's own brightness for
    /// `scheme = "terminal"`, and the theme when there is no `scheme`.
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
            scheme: None,
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
        let mut palette = match (self.effective_scheme(), self.base) {
            (SchemeChoice::Dark, _) | (SchemeChoice::Terminal, Base::Dark) => Palette::DARK,
            (SchemeChoice::Light, _) | (SchemeChoice::Terminal, Base::Light) => Palette::LIGHT,
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

    /// The scheme in force: the one written, or the one `base` says when none is.
    pub const fn effective_scheme(&self) -> SchemeChoice {
        match (self.scheme, self.base) {
            (Some(scheme), _) => scheme,
            (None, Base::Dark) => SchemeChoice::Dark,
            (None, Base::Light) => SchemeChoice::Light,
        }
    }

    /// The colours to paint the screen with, `None` when the terminal's own are
    /// kept.
    pub const fn scheme(&self) -> Option<Scheme> {
        match self.effective_scheme() {
            SchemeChoice::Terminal => None,
            SchemeChoice::Dark => Some(Scheme::DARK),
            SchemeChoice::Light => Some(Scheme::LIGHT),
        }
    }

    pub fn color(&self) -> Color {
        self.accent.unwrap_or_else(|| self.preset.color())
    }
}
