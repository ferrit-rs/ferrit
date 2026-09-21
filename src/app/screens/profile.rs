//! Settings and repository activity, rendered together in the profile drawer.

mod activity;
mod settings;

use crate::app::theme;
use crate::app::theme_config::ThemeConfig;
use crate::components::tui_overlay::state::OverlayState;
use crate::components::ui::color_picker::{ColorPickerDisplay, ColorPickerHitAreas};
use crate::components::ui::drawer::Drawer;
use crate::components::ui::scroll_bar::ScrollBar;
use crate::domain::profile::Profile;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

const PROFILE_BUTTON_HEIGHT: u16 = 1;

pub(super) fn draw_author(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut OverlayState,
    profile: &Profile,
    scroll: &mut usize,
    config: &ThemeConfig,
    theme_editing: bool,
    rgb_channel: usize,
    palette_open: bool,
    palette_selected: usize,
    picker_display: ColorPickerDisplay,
    theme_dirty: bool,
    picker_hit_areas: &mut ColorPickerHitAreas,
    selected_author: Option<&crate::domain::profile::settings::Identity>,
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

    let settings_view = settings::lines(
        &profile.settings,
        config,
        theme_editing,
        rgb_channel,
        palette_open,
        palette_selected,
        picker_display,
        theme_dirty,
        body.width,
        selected_author,
    );
    let grid_start = settings_view.picker_grid_line;
    let grid_rows = settings_view.picker_grid_metrics.rows;
    let save_line = settings_view.save_button_line;
    let save_width = settings_view.save_button_width;
    let mut lines = settings_view.lines;
    lines.extend(activity::lines(&profile.activity, body.width));

    let content_length = lines.len();
    let viewport = usize::from(body.height);
    let max_scroll = content_length.saturating_sub(viewport);
    *scroll = (*scroll).min(max_scroll);
    *picker_hit_areas = hit_areas(body, *scroll, grid_start, grid_rows, save_line, save_width);
    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(*scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
    ScrollBar::new(content_length, viewport, *scroll)
        .style(Style::new().fg(theme::IDLE))
        .render(frame, track);
    frame.render_widget(
        Paragraph::new(Line::styled(
            "1–9 select author · 0 Git author · p picker · click color · s save · Esc close",
            Style::new().fg(theme::IDLE),
        )),
        hint,
    );
}

fn hit_areas(
    body: Rect,
    scroll: usize,
    grid_start: usize,
    grid_rows: usize,
    save_line: usize,
    save_width: u16,
) -> ColorPickerHitAreas {
    let viewport_start = scroll;
    let viewport_end = scroll.saturating_add(usize::from(body.height));
    let grid_end = grid_start.saturating_add(grid_rows);
    let visible_grid_start = grid_start.max(viewport_start);
    let visible_grid_end = grid_end.min(viewport_end);
    let grid = if visible_grid_start < visible_grid_end {
        Rect::new(
            body.x,
            body.y.saturating_add(
                u16::try_from(visible_grid_start - viewport_start).unwrap_or(u16::MAX),
            ),
            body.width,
            u16::try_from(visible_grid_end - visible_grid_start).unwrap_or(u16::MAX),
        )
    } else {
        Rect::ZERO
    };
    let save_button = if (viewport_start..viewport_end).contains(&save_line) {
        Rect::new(
            body.x,
            body.y
                .saturating_add(u16::try_from(save_line - viewport_start).unwrap_or(u16::MAX)),
            save_width.min(body.width),
            PROFILE_BUTTON_HEIGHT,
        )
    } else {
        Rect::ZERO
    };
    ColorPickerHitAreas {
        grid,
        grid_first_row: visible_grid_start.saturating_sub(grid_start),
        save_button,
    }
}
