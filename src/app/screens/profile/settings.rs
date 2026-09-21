//! Git and Ferrit settings shown in the profile drawer.

use crate::app::theme;
use crate::app::theme_config::ThemeConfig;
use crate::components::ui::color_picker::{
    ColorPicker, ColorPickerDisplay, ColorPickerGridMetrics, rgb,
};
use crate::components::ui::radio_card::RadioCard;
use crate::components::ui::separator::Separator;
use crate::domain::profile::settings::{Identity, IdentitySource, Settings};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

const RGB_CHANNEL_LABELS: [&str; crate::app::theme_config::RGB_CHANNEL_COUNT] = ["R", "G", "B"];
const RGB_VALUE_WIDTH: usize = 3;
const SECTION_SEPARATOR_MARGIN_X: u16 = 1;
const SECTION_SEPARATOR_MARGIN_Y: u16 = 1;
pub(super) const SAVE_BUTTON_LABEL: &str = "[ Save ]";
const SAVED_BUTTON_LABEL: &str = "[ Saved ]";
const AUTHOR_SELECTION_KEYS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

pub(super) struct SettingsView {
    pub(super) lines: Vec<Line<'static>>,
    pub(super) picker_grid_line: usize,
    pub(super) picker_grid_metrics: ColorPickerGridMetrics,
    pub(super) save_button_line: usize,
    pub(super) save_button_width: u16,
    pub(super) author_cards: Vec<(usize, u16)>,
}

pub(super) fn lines(
    settings: &Settings,
    config: &ThemeConfig,
    editing: bool,
    channel: usize,
    palette_open: bool,
    palette_selected: usize,
    picker_display: ColorPickerDisplay,
    theme_dirty: bool,
    width: u16,
    selected_author: Option<&Identity>,
) -> SettingsView {
    let divider = |label| {
        Separator::new(label)
            .style(Style::new().fg(theme::IDLE))
            .margin_x(SECTION_SEPARATOR_MARGIN_X)
            .margin_y(SECTION_SEPARATOR_MARGIN_Y)
            .lines(width)
    };
    let mut lines = divider("Global Git users · selection applies to Ferrit commits only");
    let active_identity = selected_author.or(settings.effective_identity.as_ref());
    let source = if selected_author.is_some() {
        "Ferrit selection"
    } else {
        match settings.identity_source {
            IdentitySource::Repository => "Repository config",
            IdentitySource::Global => "Global config",
            IdentitySource::System => "System config",
            IdentitySource::Unset => "Not configured",
        }
    };
    lines.push(Line::styled(
        format!(
            "In use ({source}): {}",
            active_identity.map_or("Not configured", |identity| identity.name.as_str())
        ),
        Style::new().add_modifier(Modifier::BOLD),
    ));
    if let Some(identity) = active_identity {
        lines.push(Line::styled(
            identity
                .email
                .clone()
                .unwrap_or_else(|| "Email not configured".to_owned()),
            Style::new().fg(theme::IDLE),
        ));
    }
    let available = settings.available_identities();
    let mut author_cards = Vec::with_capacity(available.len());
    if available.is_empty() {
        lines.push(Line::styled(
            "No configured identities",
            Style::new().fg(theme::IDLE),
        ));
    } else {
        for (index, identity) in available.iter().enumerate() {
            let selected = selected_author.map_or(
                settings.effective_identity.as_ref() == Some(identity),
                |active| active == identity,
            );
            let key = AUTHOR_SELECTION_KEYS.get(index).copied().unwrap_or("-");
            let email = identity.email.as_deref().unwrap_or("Email not configured");
            let card = RadioCard::new(identity.name.clone(), email)
                .key(key)
                .selected(selected)
                .accent(config.color())
                .w_fit(width);
            author_cards.push((lines.len(), card.fitted_width()));
            lines.extend(card.lines());
        }
    }
    lines.push(Line::styled(
        "0 · use Git config identity",
        Style::new().fg(theme::IDLE),
    ));
    lines.extend(divider("Ferrit settings"));
    lines.push(Line::from(format!("Theme: {}", config.preset.name())));
    let picker_lines = ColorPicker::new(config.color())
        .selected(palette_selected)
        .active(palette_open)
        .display(picker_display)
        .lines();
    let picker_grid_line = lines.len() + 1;
    let picker_grid_metrics = crate::components::ui::color_picker::grid_metrics(picker_display);
    lines.extend(picker_lines);
    let (r, g, b) = rgb(config.color());
    lines.push(Line::styled(
        if editing {
            format!(
                "RGB: [R {r:0RGB_VALUE_WIDTH$}] [G {g:0RGB_VALUE_WIDTH$}] [B {b:0RGB_VALUE_WIDTH$}] · channel {}",
                RGB_CHANNEL_LABELS
                    .get(channel)
                    .copied()
                    .unwrap_or(RGB_CHANNEL_LABELS[crate::app::theme_config::RGB_BLUE_CHANNEL])
            )
        } else {
            if palette_open {
                "arrows preview · v view · s save".to_owned()
            } else {
                "p picker · e edit RGB · t preset · s save".to_owned()
            }
        },
        Style::new().fg(theme::IDLE),
    ));
    let save_button_line = lines.len();
    let save_button_label = if theme_dirty {
        SAVE_BUTTON_LABEL
    } else {
        SAVED_BUTTON_LABEL
    };
    let save_button_width =
        u16::try_from(Line::from(save_button_label).width()).unwrap_or(u16::MAX);
    lines.push(Line::styled(
        save_button_label,
        Style::new()
            .fg(if theme_dirty {
                config.color()
            } else {
                theme::IDLE
            })
            .add_modifier(if theme_dirty {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
    ));
    SettingsView {
        lines,
        picker_grid_line,
        picker_grid_metrics,
        save_button_line,
        save_button_width,
        author_cards,
    }
}
