//! Settings and repository activity, rendered together in the profile drawer.

mod activity;
mod settings;

use crate::app::theme;
use crate::app::theme_config::ThemeConfig;
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
    config: &ThemeConfig,
    theme_editing: bool,
    rgb_channel: usize,
) {
    let Some(inner) = Drawer::new(state, " Profile ")
        .width(Constraint::Percentage(75))
        .border_style(Style::new().fg(config.color()))
        .render(frame, area)
    else {
        return;
    };
    let [content, hint] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    let [body, track] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(content);

    let mut lines = settings::lines(&profile.settings, config, theme_editing, rgb_channel);
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
            "t preset · e edit RGB · Tab channel · ↑/↓ adjust · Esc close",
            Style::new().fg(theme::IDLE),
        )),
        hint,
    );
}
