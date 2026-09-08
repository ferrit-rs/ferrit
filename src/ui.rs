//! `draw(frame, &app)`: the whole screen, top-level layout down to widgets.
//!
//! Pure rendering. It reads `App` and `mock`, never mutates, never touches a
//! terminal, so `tests/render.rs` can call it against a `TestBackend`.
//! Colours come from `theme`.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Clear, List, ListState, Paragraph, Wrap};
use ratatui_image::{Resize, StatefulImage};

use crate::app::{App, Pane, PANES};
use crate::image::preview::Preview;
use crate::{mock, theme};

/// Render the full screen for the current `App` state.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let [content, log, keybar] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .areas(area);

    // lazygit's default `sidePanelWidth: 0.3333`: the left column takes a third
    // of the width, floored so it stays usable on a narrow terminal.
    let side = (area.width / 3).max(24);
    let [left, right] =
        Layout::horizontal([Constraint::Length(side), Constraint::Min(0)]).areas(content);

    let show_help = app.show_help;
    draw_left_column(frame, app, left);
    draw_right_pane(frame, app, right);
    draw_command_log(frame, log);
    draw_keybar(frame, keybar);

    if show_help {
        draw_help(frame, area);
    }
}

/// Colour each pane's rows by what they mean. Status and Files come from the
/// live snapshot on `App`; the rest are still mock.
fn pane_lines(app: &App, pane: Pane) -> Vec<Line<'static>> {
    match pane {
        Pane::Status => app.status_lines(),
        Pane::Files => app.file_lines(),
        Pane::Branches => app.branch_lines(),
        Pane::Commits => app.commit_lines(),
        Pane::Stash => app.stash_lines(),
    }
}

fn draw_left_column(frame: &mut Frame, app: &App, area: Rect) {
    let rows: [Rect; 5] = Layout::vertical([
        Constraint::Length(4), // Status: header only
        Constraint::Min(3),    // Files
        Constraint::Min(3),    // Branches
        Constraint::Min(3),    // Commits
        Constraint::Length(4), // Stash
    ])
    .areas(area);

    for (i, &pane) in PANES.iter().enumerate() {
        let focused = app.focus == pane;
        let border = if focused {
            Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme::IDLE)
        };
        let title = Line::styled(
            format!(" {} ", pane.title()),
            if focused {
                Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme::IDLE)
            },
        );

        let mut block = Block::bordered().title(title).border_style(border);
        if let Some((cur, total)) = app.counter(pane) {
            block = block.title_bottom(theme::counter_line(cur, total));
        }

        let list = List::new(pane_lines(app, pane))
            .block(block)
            .highlight_style(theme::selection_style(focused));

        let mut state = ListState::default();
        let row_ct = app.row_count(pane);
        if row_ct > 0 {
            state.select(Some(app.selected(pane).min(row_ct - 1)));
        }

        frame.render_stateful_widget(list, rows[i], &mut state);
    }
}

fn draw_right_pane(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD);
    let idle = Style::new().fg(theme::IDLE);
    let right_title = app.focus.right_title();

    // An image selection takes over the right pane; otherwise it is mock text.
    match app.preview_mut() {
        Preview::Image(proto) => {
            // Same shape as `render_resized_image` in the ratatui-image demo:
            // draw the border, then hand `StatefulImage` the inner area and a
            // `&mut StatefulProtocol` so it resizes + re-encodes to fit.
            let block = Block::bordered()
                .title(Line::styled(" Preview ", focused))
                .border_style(idle);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            frame.render_stateful_widget(
                StatefulImage::new().resize(Resize::Fit(None)),
                inner,
                proto.as_mut(),
            );
            return;
        }
        Preview::Note(msg) => {
            // Blank every cell first: if the previous frame was an image, its
            // sixel / iTerm2 pixels sit under these cells and a short paragraph
            // would not overwrite the rows below it.
            frame.render_widget(Clear, area);
            let panel = Paragraph::new(msg.as_str())
                .block(
                    Block::bordered()
                        .title(Line::styled(right_title, focused))
                        .border_style(idle),
                )
                .wrap(Wrap { trim: false });
            frame.render_widget(panel, area);
            return;
        }
        Preview::None => {}
    }

    // Same reason as the `Note` branch: clear any leftover graphics pixels
    // from a previous image frame before drawing the (often short) text pane.
    frame.render_widget(Clear, area);

    let body = match app.focus {
        Pane::Status => mock::RIGHT_STATUS,
        Pane::Files => mock::RIGHT_DIFF,
        Pane::Branches => mock::RIGHT_LOG,
        Pane::Commits => mock::RIGHT_COMMIT,
        Pane::Stash => mock::RIGHT_STASH,
    };

    let text: Text = match app.focus {
        Pane::Files | Pane::Branches | Pane::Commits => theme::diff_text(body),
        _ => body.into(),
    };

    let panel = Paragraph::new(text)
        .block(
            Block::bordered()
                .title(Line::styled(right_title, focused))
                .border_style(Style::new().fg(theme::IDLE)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

fn draw_command_log(frame: &mut Frame, area: Rect) {
    let lines: Vec<Line> = mock::COMMAND_LOG.iter().map(|s| theme::log_line(s)).collect();
    let panel = Paragraph::new(lines).block(
        Block::bordered()
            .title(Line::styled(" command log ", Style::new().fg(theme::IDLE)))
            .border_style(Style::new().fg(theme::IDLE)),
    );
    frame.render_widget(panel, area);
}

fn draw_keybar(frame: &mut Frame, area: Rect) {
    frame.render_widget(Paragraph::new(theme::keybar_line(mock::KEYBAR)), area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let width = 46.min(area.width);
    let height = 11.min(area.height);
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };

    let overlay = Paragraph::new(mock::HELP).block(
        Block::bordered()
            .title(Line::styled(
                " keybindings ",
                Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD),
            ))
            .border_style(Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD)),
    );

    frame.render_widget(Clear, rect);
    frame.render_widget(overlay, rect);
}
