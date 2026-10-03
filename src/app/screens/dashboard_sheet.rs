//! The dashboard as a sheet (`docs/PLAN_19_DASHBOARD_SHEET.md`): the page of
//! `screens/dashboard.rs` drawn into the drawer's inner area, 90 % wide so that
//! its two columns fit on the usual wide terminals.

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;

use super::{charts_mode_from_env, dashboard, unix_now};
use crate::app::App;
use crate::components::ui::chart_palette::ChartPalette;
use crate::components::ui::drawer::Drawer;

/// How much of the width the drawer takes.
const WIDTH_PERCENT: u16 = 90;

/// Draw the drawer and the page in it, over `area`. The page's scroll is clamped
/// to what it can scroll.
pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, app: &mut App) {
    let accent = app.theme_config.color();
    let Some(inner) = Drawer::new(&mut app.sheet_overlay, " Dashboard ")
        .width(Constraint::Percentage(WIDTH_PERCENT))
        .border_style(Style::new().fg(accent))
        .render(frame, area)
    else {
        return;
    };
    let view = dashboard::View {
        stats: app.dashboard().stats(),
        repo: &app.repo_name,
        branch: &app.header.branch,
        colors: ChartPalette {
            density: std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()),
            ..ChartPalette::for_palette(&app.palette())
        },
        mode: charts_mode_from_env(),
        show_counts: app.dashboard().show_counts(),
        computing: app.dashboard().computing(),
        churn_pending: app.dashboard().churn_pending(),
        error: app.dashboard().error(),
        scroll: app.dashboard().scroll(),
        now: unix_now(),
    };
    let max_scroll = dashboard::draw(frame, inner, &view);
    app.clamp_dashboard_scroll(max_scroll);
}
