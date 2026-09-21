//! Git and Ferrit settings shown in the profile drawer.

use crate::app::theme;
use crate::app::theme_config::ThemeConfig;
use crate::components::ui::color_picker::ColorPicker;
use crate::components::ui::separator::Separator;
use crate::domain::profile::{Identity, Settings};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

const RGB_CHANNEL_LABELS: [&str; crate::app::theme_config::RGB_CHANNEL_COUNT] = ["R", "G", "B"];
const RGB_VALUE_WIDTH: usize = 3;
const SECTION_SEPARATOR_MARGIN_X: u16 = 1;
const SECTION_SEPARATOR_MARGIN_Y: u16 = 1;

pub(super) fn lines(
    settings: &Settings,
    config: &ThemeConfig,
    editing: bool,
    channel: usize,
    palette_open: bool,
    palette_selected: usize,
    width: u16,
) -> Vec<Line<'static>> {
    let divider = |label| {
        Separator::new(label)
            .style(Style::new().fg(theme::IDLE))
            .margin_x(SECTION_SEPARATOR_MARGIN_X)
            .margin_y(SECTION_SEPARATOR_MARGIN_Y)
            .lines(width)
    };
    let mut lines = divider("Git identities");
    append_identity(&mut lines, "Global", settings.global_identity.as_ref());
    append_identity(
        &mut lines,
        "Repository",
        settings.repository_identity.as_ref(),
    );
    lines.extend(divider("Ferrit settings"));
    lines.push(Line::from(format!("Theme: {}", config.preset.name())));
    lines.extend(
        ColorPicker::new(config.color())
            .selected(palette_selected)
            .active(palette_open)
            .lines(),
    );
    let (r, g, b) = crate::components::ui::color_picker::rgb(config.color());
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
            "p palette · t preset · e edit RGB".to_owned()
        },
        Style::new().fg(theme::IDLE),
    ));
    lines.extend(divider("Effective author identities"));
    if settings.effective_identities.is_empty() {
        lines.push(Line::from("Not configured"));
    } else {
        for identity in &settings.effective_identities {
            lines.push(Line::styled(
                identity.name.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::styled(
                identity
                    .email
                    .clone()
                    .unwrap_or_else(|| "Email not configured".to_owned()),
                Style::new().fg(theme::IDLE),
            ));
        }
    }
    lines
}

fn append_identity(lines: &mut Vec<Line<'static>>, label: &str, identity: Option<&Identity>) {
    lines.push(Line::styled(
        label.to_owned(),
        Style::new().add_modifier(Modifier::BOLD),
    ));
    if let Some(identity) = identity {
        lines.push(Line::from(identity.name.clone()));
        lines.push(Line::styled(
            identity
                .email
                .clone()
                .unwrap_or_else(|| "Email not configured".to_owned()),
            Style::new().fg(theme::IDLE),
        ));
    } else {
        lines.push(Line::styled("Not configured", Style::new().fg(theme::IDLE)));
    }
}
