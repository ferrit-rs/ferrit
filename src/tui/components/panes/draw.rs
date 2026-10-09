//! The five left panes: `draw`.

use crate::tui::components::panes::nav::PANES;
use crate::tui::components::panes::nav::Pane;
use crate::tui::draw::Landed;
use crate::tui::row_lines;
use crate::tui::scene::Scene;
use crate::tui::widgets::chrome::PaneList;
use crate::tui::widgets::chrome::Panel;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

/// Colour each pane's rows by what they mean. Status and Files come from the
/// live snapshot on `App`; the rest are still mock.
pub(crate) fn pane_lines(app: &Scene<'_>, pane: Pane) -> Vec<Line<'static>> {
    match pane {
        Pane::Status => app.status_lines(),
        Pane::Files => app.file_lines(),
        Pane::Branches => app.branch_lines(),
        Pane::Commits => app.commit_lines(),
        Pane::Stash => app.stash_lines(),
    }
}

pub(crate) fn draw_left_column(
    frame: &mut Frame<'_>,
    app: &Scene<'_>,
    landed: &mut Landed,
    area: Rect,
) {
    let palette = &app.palette();
    // Status only ever shows 1 line, or 2 when there's a conflict to report
    // (`App::status_lines`): sized to that instead of a flat 4, so a short
    // terminal doesn't pay for a conflict line that (almost always) isn't
    // there.
    let status_height = u16::try_from(app.status_lines().len() + 2).unwrap_or(4);
    let [status_row, accordion_area] =
        Layout::vertical([Constraint::Length(status_height), Constraint::Min(0)]).areas(area);

    // lazygit's `expandFocusedSidePanel` accordion: the focused pane claims a
    // weighted majority of the space, everyone else shares what's left.
    // Weighted rather than "a fixed floor each, 100% of the leftover to
    // focus": that scheme gave a dramatic boost in a roomy terminal but fell
    // back to a perfectly even split — no accordion at all — the moment
    // there wasn't room for every pane's floor, which is exactly the short
    // terminal where showing one pane clearly, lazygit-style, matters most.
    // `FOCUS_WEIGHT` shares go to the focused pane, 1 share to each other;
    // when the focus is Status (outside this group), there is no pane to
    // boost, so every pane gets 1 share (an even split, not left blank).
    // Ratatui's `Fill`/`Min` mix is order-sensitive at small heights (it can
    // starve the boosted pane below its neighbours), so the split is
    // computed by hand rather than left to the `Layout` solver.
    const DYNAMIC: [Pane; 4] = [Pane::Files, Pane::Branches, Pane::Commits, Pane::Stash];
    const FOCUS_WEIGHT: u16 = 4;
    const MIN_HEIGHT: u16 = 2; // a collapsed but still-bordered box: no room for a content row
    let focus_index = DYNAMIC.iter().position(|&p| p == app.nav.focus);

    let weights: [u16; 4] = focus_index.map_or([1; 4], |idx| {
        std::array::from_fn(|i| if i == idx { FOCUS_WEIGHT } else { 1 })
    });
    let total_weight: u16 = weights.iter().sum();
    let mut heights: [u16; 4] = std::array::from_fn(|i| {
        let weight = weights.get(i).copied().unwrap_or(1);
        (accordion_area.height * weight / total_weight).max(MIN_HEIGHT)
    });

    // The weighted shares rarely sum to exactly `accordion_area.height`,
    // especially once every pane is floored to `MIN_HEIGHT`. Round-robin the
    // remainder (or the overshoot) so the total always matches exactly,
    // never taking a pane below 0.
    let mut diff = i32::from(accordion_area.height) - i32::from(heights.iter().sum::<u16>());
    let mut i = 0;
    while diff != 0 {
        let Some(h) = heights.get_mut(i) else { break };
        if diff > 0 {
            *h += 1;
            diff -= 1;
        } else if *h > 0 {
            *h -= 1;
            diff += 1;
        }
        i = (i + 1) % heights.len();
    }

    let mut rows = [
        status_row,
        Rect::default(),
        Rect::default(),
        Rect::default(),
        Rect::default(),
    ];
    let mut y = accordion_area.y;
    for (i, &h) in heights.iter().enumerate() {
        if let Some(row) = rows.get_mut(i + 1) {
            *row = Rect {
                x: accordion_area.x,
                y,
                width: accordion_area.width,
                height: h,
            };
        }
        y += h;
    }

    for (&pane, &row) in PANES.iter().zip(&rows) {
        // Remembered for click routing: written before the list body is
        // read, so this `&mut` borrow never overlaps the `&self` one below.
        landed.left.push((pane, row));

        let focused = app.nav.focus == pane && !app.right_focused();
        let border = if focused {
            Style::new()
                .fg(app.theme.config.color())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(palette.idle)
        };
        let title_text = if pane == Pane::Branches {
            app.branches_title()
        } else if pane == Pane::Commits {
            app.commits_title()
        } else {
            pane.title().to_owned()
        };
        let title = Line::styled(
            format!(" {title_text} "),
            if focused {
                Style::new()
                    .fg(app.theme.config.color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(palette.idle)
            },
        );

        let mut panel = Panel::new().title(title).border_style(border);
        if let Some((cur, total)) = app.counter(pane) {
            panel = panel.bottom_title(row_lines::counter_line(palette, cur, total));
        }
        let block = panel.block();

        let row_ct = app.row_count(pane);
        let mut lines = pane_lines(app, pane);
        let mut highlight = row_lines::selection_style(palette, focused);
        if pane == Pane::Files && focused {
            // Files rows carry a staging colour that the bar must not repaint.
            highlight.fg = None;
            if let Some(line) = lines.get_mut(app.selected(pane)) {
                row_lines::keep_colours_on_selection(palette, line);
            }
        }
        let detached = app.view_detached(pane);
        let offset = PaneList::new(lines, block)
            .detached(detached)
            .selected((row_ct > 0).then(|| app.selected(pane).min(row_ct - 1)))
            .offset(app.list_offset(pane))
            .highlight_style(highlight)
            .scrollbar_style(border)
            .render(frame, row);
        // Ratatui may have moved the offset to keep the selection on screen;
        // copy it back so a click in a scrolled list maps to the right row.
        landed.list_offset.push((pane, offset));
    }
}
