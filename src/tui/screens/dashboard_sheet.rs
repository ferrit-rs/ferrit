//! The dashboard as a sheet (`docs/PLAN_19_DASHBOARD_SHEET.md`): the page of
//! `screens/dashboard.rs` drawn into the drawer's inner area. The drawer is as wide
//! as the page ever gets (it stops at `MAX_WIDTH`, so a wider drawer would only add
//! margins) and at most 95 % of the terminal, so the panes stay in sight.

use super::landed::Landed;
use crate::tui::state::render_state::RenderState;
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::Style;

use super::dashboard::Chrome;
use super::{dashboard, unix_now};
use crate::tui::App;
use crate::tui::widgets::chart_palette::ChartPalette;
use crate::tui::widgets::chart_palette::charts_mode_from_env;
use crate::tui::widgets::drawer::Drawer;

/// The most of the terminal's width the drawer takes, in percent.
const MAX_PERCENT: u16 = 95;

/// Draw the drawer and the page in it, over `area`. The page's scroll is clamped
/// to what it can scroll.
pub(crate) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    render: &mut RenderState,
    landed: &mut Landed,
) {
    let accent = app.theme.config.color();
    // The page plus the drawer's two border columns, no more.
    let width = (dashboard::MAX_WIDTH + 2).min(area.width.saturating_mul(MAX_PERCENT) / 100);
    let Some(inner) = Drawer::new(&mut render.sheet, " Dashboard ")
        .width(Constraint::Length(width))
        .border_style(Style::new().fg(accent))
        .render(frame, area)
    else {
        return;
    };
    let view = dashboard::View {
        stats: app.dashboard().stats(),
        repo: &app.repo_name,
        branch: &app.snapshot.header.branch,
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
        chrome: Chrome::Bare,
        now: unix_now(),
    };
    let max_scroll = dashboard::draw(frame, inner, &view);
    landed.dashboard_max_scroll = Some(max_scroll);
}
