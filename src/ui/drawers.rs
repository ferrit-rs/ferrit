//! Drawer body content, rendered inside the reusable drawer shell.

use crate::components::tui_overlay::OverlayState;
use crate::components::ui::drawer::Drawer;
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

pub(super) fn draw_author(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut OverlayState,
    identities: &[crate::git::model::UserIdentity],
) {
    let Some(inner) = Drawer::new(state, " Git identity ").render(frame, area) else {
        return;
    };
    let [heading, list, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new("Configured author identities").style(Style::new().fg(theme::IDLE)),
        heading,
    );
    let lines = if identities.is_empty() {
        vec![Line::from("Not configured")]
    } else {
        identities
            .iter()
            .flat_map(|identity| {
                [
                    Line::styled(
                        identity.name.as_str(),
                        Style::new().add_modifier(Modifier::BOLD),
                    ),
                    Line::styled(
                        identity.email.as_deref().unwrap_or("Email not configured"),
                        Style::new().fg(theme::IDLE),
                    ),
                    Line::from(""),
                ]
            })
            .collect()
    };
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), list);
    frame.render_widget(
        Paragraph::new(Line::styled(
            "Esc / click outside to close",
            Style::new().fg(theme::IDLE),
        )),
        hint,
    );
}
