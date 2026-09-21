//! Git and Ferrit settings shown in the profile drawer.

use crate::app::theme;
use crate::domain::profile::{Identity, Settings};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, settings: &Settings) {
    let mut lines = vec![Line::styled("Git identities", Style::new().fg(theme::IDLE))];
    append_identity(&mut lines, "Global", settings.global_identity.as_ref());
    append_identity(
        &mut lines,
        "Repository",
        settings.repository_identity.as_ref(),
    );
    lines.push(Line::from(""));
    lines.push(Line::styled("Ferrit", Style::new().fg(theme::IDLE)));
    lines.push(Line::from("Theme: Green (default)"));
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
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
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
