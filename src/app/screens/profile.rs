//! Drawer body content, rendered inside reusable drawer shells.

mod activity;
mod settings;

use crate::app::ProfileTab;
use crate::app::theme;
use crate::components::tui_overlay::OverlayState;
use crate::components::ui::drawer::Drawer;
use crate::domain::profile::Profile;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(super) fn draw_author(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut OverlayState,
    tab: ProfileTab,
    profile: &Profile,
) {
    let width = if tab == ProfileTab::Activity {
        Constraint::Percentage(75)
    } else {
        Constraint::Percentage(50)
    };
    let Some(inner) = Drawer::new(state, " Profile ")
        .width(width)
        .render(frame, area)
    else {
        return;
    };
    let [tabs, body, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    let tab_style = |selected| {
        if selected {
            Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme::IDLE)
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Settings ", tab_style(tab == ProfileTab::Settings)),
            Span::raw("  "),
            Span::styled(" Activity ", tab_style(tab == ProfileTab::Activity)),
        ])),
        tabs,
    );
    match tab {
        ProfileTab::Settings => settings::draw(frame, body, &profile.settings),
        ProfileTab::Activity => activity::draw(frame, body, &profile.activity),
    }
    frame.render_widget(
        Paragraph::new(Line::styled(
            "Tab / ← → switch · Esc / outside close",
            Style::new().fg(theme::IDLE),
        )),
        hint,
    );
}
