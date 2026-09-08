//! `draw(frame, &app)`: the whole screen, top-level layout down to widgets.
//!
//! Pure rendering. It reads `App` and `mock`, never mutates, never touches a
//! terminal, so `tests/render.rs` can call it against a `TestBackend`.
//! Colours come from `theme`.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{
    Block, Clear, List, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};
use ratatui_image::{Resize, StatefulImage};

use crate::app::{App, DiffView, PANES, Pane};
use crate::image::preview::Preview;
use crate::{mock, theme};

/// Render the full screen for the current `App` state.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
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

fn draw_left_column(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let rows: [Rect; 5] = Layout::vertical([
        Constraint::Length(4), // Status: header only
        Constraint::Min(3),    // Files
        Constraint::Min(3),    // Branches
        Constraint::Min(3),    // Commits
        Constraint::Length(4), // Stash
    ])
    .areas(area);

    for (&pane, &row) in PANES.iter().zip(&rows) {
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

        frame.render_stateful_widget(list, row, &mut state);
    }
}

fn draw_right_pane(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    // Remembered for mouse-wheel routing: a wheel event over this rect scrolls
    // the diff, one over the left column moves the selection.
    app.set_right_area(area);

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
        },
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
        },
        Preview::None => {},
    }

    // Same reason as the `Note` branch: clear any leftover graphics pixels
    // from a previous image frame before drawing the (often short) text pane.
    frame.render_widget(Clear, area);

    let block = Block::bordered()
        .title(Line::styled(right_title, focused))
        .border_style(Style::new().fg(theme::IDLE));

    // Real `git diff` / `git show` output: git-native colouring, vertical
    // scroll from `app.right_scroll()`, a reverse-highlight on the hunk / file
    // header a `]` / `[` jump last landed on, and a scrollbar when it overflows.
    let scroll = app.right_scroll();
    if matches!(app.diff_view(), DiffView::Files(_) | DiffView::Commit(..)) {
        let (text, total) = match app.diff_view() {
            DiffView::Files(diff) | DiffView::Commit(_, diff) => {
                let anchors = match app.diff_view() {
                    DiffView::Commit(..) => diff.file_lines(),
                    _ => diff.hunk_lines(),
                };
                let focus = anchors.iter().position(|&l| l == scroll);
                (theme::render_diff(diff, focus), diff.text.lines().count())
            },
            #[expect(
                clippy::unreachable,
                reason = "the enclosing `matches!` guard admits only Files/Commit"
            )]
            _ => unreachable!("guarded by the matches! above"),
        };
        let inner = block.inner(area);
        let panel = Paragraph::new(text)
            .block(block)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0));
        frame.render_widget(panel, area);
        if total > inner.height as usize {
            let mut state = ScrollbarState::new(total).position(scroll);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut state,
            );
        }
        app.set_right_viewport(inner.height as usize);
        return;
    }

    if let DiffView::Note(msg) = app.diff_view() {
        let panel = Paragraph::new(Line::styled(
            msg.clone(),
            Style::new().fg(theme::IDLE).add_modifier(Modifier::DIM),
        ))
        .block(block)
        .wrap(Wrap { trim: false });
        frame.render_widget(panel, area);
        return;
    }

    // No repo (mock) or a pane with no diff: the sample text.
    let body = match app.focus {
        Pane::Status => mock::RIGHT_STATUS,
        Pane::Files => mock::RIGHT_DIFF,
        Pane::Branches => mock::RIGHT_LOG,
        Pane::Commits => mock::RIGHT_COMMIT,
        Pane::Stash => mock::RIGHT_STASH,
    };

    let text: Text<'_> = match app.focus {
        Pane::Files | Pane::Branches | Pane::Commits => theme::diff_lines(body, None),
        _ => body.into(),
    };

    let panel = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

fn draw_command_log(frame: &mut Frame<'_>, area: Rect) {
    let lines: Vec<Line<'_>> = mock::COMMAND_LOG
        .iter()
        .map(|s| theme::log_line(s))
        .collect();
    let panel = Paragraph::new(lines).block(
        Block::bordered()
            .title(Line::styled(" command log ", Style::new().fg(theme::IDLE)))
            .border_style(Style::new().fg(theme::IDLE)),
    );
    frame.render_widget(panel, area);
}

fn draw_keybar(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(Paragraph::new(theme::keybar_line(mock::KEYBAR)), area);
}

fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let width = 55.min(area.width);
    let height = 15.min(area.height);
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
