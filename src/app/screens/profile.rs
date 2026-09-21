//! Settings and repository activity, rendered together in the profile drawer.

mod activity;
mod settings;

use crate::app::theme;
use crate::components::tui_overlay::OverlayState;
use crate::components::ui::drawer::Drawer;
use crate::components::ui::scroll_bar::ScrollBar;
use crate::domain::profile::Profile;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

pub(super) fn draw_author(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut OverlayState,
    profile: &Profile,
    scroll: &mut usize,
) {
    let Some(inner) = Drawer::new(state, " Profile ")
        .width(Constraint::Percentage(75))
        .render(frame, area)
    else {
        return;
    };
    let [content, hint] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    let [body, track] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(content);

    let mut lines = settings::lines(&profile.settings);
    lines.extend(activity::lines(&profile.activity, body.width));

    let content_length = lines.len();
    let viewport = usize::from(body.height);
    let max_scroll = content_length.saturating_sub(viewport);
    *scroll = (*scroll).min(max_scroll);
    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(*scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
    ScrollBar::new(content_length, viewport, *scroll)
        .style(Style::new().fg(theme::IDLE))
        .render(frame, track);
    frame.render_widget(
        Paragraph::new(Line::styled(
            "↑/↓ or j/k scroll · PgUp/PgDn page · Home/End · Esc close",
            Style::new().fg(theme::IDLE),
        )),
        hint,
    );
}
