//! Git and Ferrit settings shown in the profile drawer.

use crate::app::theme;
use crate::app::theme_config::ThemeConfig;
use crate::domain::profile::{Identity, Settings};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

pub(super) fn lines(
    settings: &Settings,
    config: &ThemeConfig,
    editing: bool,
    channel: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled("Git identities", Style::new().fg(theme::IDLE))];
    append_identity(&mut lines, "Global", settings.global_identity.as_ref());
    append_identity(
        &mut lines,
        "Repository",
        settings.repository_identity.as_ref(),
    );
    lines.push(Line::from(""));
    lines.push(Line::styled("Ferrit", Style::new().fg(theme::IDLE)));
    let (r, g, b) = match config.color() {
        ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
        ratatui::style::Color::Green => (0, 255, 0),
        ratatui::style::Color::Cyan => (0, 255, 255),
        ratatui::style::Color::Magenta => (255, 0, 255),
        ratatui::style::Color::Yellow => (255, 255, 0),
        _ => (0, 255, 0),
    };
    lines.push(Line::from(format!("Theme: {}", config.preset.name())));
    lines.push(Line::styled(
        format!("Accent: #{r:02X}{g:02X}{b:02X}"),
        Style::new().fg(config.color()),
    ));
    lines.push(Line::styled(
        if editing {
            format!(
                "RGB: [R {r:03}] [G {g:03}] [B {b:03}] · channel {}",
                match channel {
                    0 => "R",
                    1 => "G",
                    _ => "B",
                }
            )
        } else {
            "t cycle preset · e edit RGB".to_owned()
        },
        Style::new().fg(theme::IDLE),
    ));
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "Effective author identities",
        Style::new().fg(theme::IDLE),
    ));
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
    lines.push(Line::from(""));
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
    lines.push(Line::from(""));
}
