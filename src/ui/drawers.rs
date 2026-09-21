//! Drawer body content, rendered inside the reusable drawer shell.

use crate::components::tui_overlay::OverlayState;
use crate::components::ui::drawer::Drawer;
use crate::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

pub(super) fn draw_author(frame: &mut Frame<'_>, area: Rect, state: &mut OverlayState, name: &str) {
    let Some(inner) = Drawer::new(state, " Git identity ").render(frame, area) else {
        return;
    };
    let [label, value, description, hint] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(Line::styled("Author name", Style::new().fg(theme::IDLE))),
        label,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            name,
            Style::new().add_modifier(Modifier::BOLD),
        )),
        value,
    );
    frame.render_widget(
        Paragraph::new("Git uses this name in commit metadata.")
            .style(Style::new().fg(theme::IDLE)),
        description,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            "Esc / click outside to close",
            Style::new().fg(theme::IDLE),
        )),
        hint,
    );
}
