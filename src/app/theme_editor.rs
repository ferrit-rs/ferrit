//! The theme being edited in the settings sheet: the colours chosen so far and
//! where the accent picker stands. Applying and saving a change stays with
//! `App` (`accent_changed`), because it also rebuilds the palette and writes
//! `config.toml`; each method here says whether the accent changed.

use ratatui::style::Color;

use super::theme_config::{
    RGB_CHANNEL_COUNT, RGB_GREEN_CHANNEL, RGB_RED_CHANNEL, ThemeConfig, ThemeMode,
};
use crate::components::ui::color_picker::{self, ColorPickerDisplay, PaletteDirection};

pub struct ThemeEditor {
    /// The theme as chosen: what the screen is painted with.
    pub config: ThemeConfig,
    pub(crate) mode: ThemeMode,
    /// Which of red, green and blue `e` edits.
    pub(crate) rgb_channel: usize,
    /// The highlighted swatch of the picker.
    pub(crate) palette_selected: usize,
    pub(crate) picker_display: ColorPickerDisplay,
}

impl ThemeEditor {
    pub(super) fn new(config: ThemeConfig) -> Self {
        let picker_display = ColorPickerDisplay::default();
        Self {
            palette_selected: color_picker::nearest_index(config.color(), picker_display),
            config,
            mode: ThemeMode::Idle,
            rgb_channel: RGB_RED_CHANNEL,
            picker_display,
        }
    }

    /// Put the highlight on the swatch nearest the current accent.
    pub(super) fn sync_picker_selection(&mut self) {
        self.palette_selected =
            color_picker::nearest_index(self.config.color(), self.picker_display);
    }

    /// The highlighted swatch becomes the accent. `true` when it did.
    fn apply_picker_selection(&mut self) -> bool {
        let Some(color) = color_picker::color_at(self.picker_display, self.palette_selected) else {
            return false;
        };
        self.config.accent = Some(color);
        true
    }

    /// Move the highlight and take that swatch as the accent.
    pub(super) fn move_palette(&mut self, direction: PaletteDirection) -> bool {
        self.palette_selected =
            color_picker::move_selection(self.palette_selected, direction, self.picker_display);
        self.apply_picker_selection()
    }

    /// Enter on the picker: take the highlighted swatch.
    pub(super) fn pick_selected(&mut self) -> bool {
        self.apply_picker_selection()
    }

    /// A click on a swatch.
    pub(super) fn select_cell(&mut self, column: usize, row: usize) -> bool {
        let Some(selected) = color_picker::selection_at(self.picker_display, column, row) else {
            return false;
        };
        self.mode = ThemeMode::Palette;
        self.palette_selected = selected;
        self.apply_picker_selection()
    }

    /// Palette grid or spectrum.
    pub(super) fn toggle_picker_display(&mut self) {
        self.picker_display = match self.picker_display {
            ColorPickerDisplay::Palette => ColorPickerDisplay::Spectrum,
            ColorPickerDisplay::Spectrum => ColorPickerDisplay::Palette,
        };
        self.sync_picker_selection();
    }

    pub(super) fn next_rgb_channel(&mut self) {
        self.rgb_channel = (self.rgb_channel + 1) % RGB_CHANNEL_COUNT;
    }

    /// Move the edited channel of the accent by `delta`, clamped to a byte.
    pub(super) fn adjust_rgb(&mut self, delta: i16) {
        let (mut r, mut g, mut b) = color_picker::rgb(self.config.color());
        let channel = match self.rgb_channel {
            RGB_RED_CHANNEL => &mut r,
            RGB_GREEN_CHANNEL => &mut g,
            _ => &mut b,
        };
        *channel = u8::try_from((i16::from(*channel) + delta).clamp(0, 255)).unwrap_or_default();
        self.config.accent = Some(Color::Rgb(r, g, b));
        self.sync_picker_selection();
    }
}
