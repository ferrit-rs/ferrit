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

use crate::app::{App, Pane, PANES};
use crate::{mock, theme};

/// Render the full screen for the current `App` state.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let [content, log, keybar] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .areas(area);

    let [left, right] =
        Layout::horizontal([Constraint::Length(28), Constraint::Min(0)]).areas(content);

    draw_left_column(frame, app, left);
    draw_right_pane(frame, app, right);
    draw_command_log(frame, log);
    draw_keybar(frame, keybar);

    if app.show_help {
        draw_help(frame, area);
    }
}

/// Colour each pane's rows by what they mean. Status and Files come from the
/// live snapshot on `App`; the rest are still mock.
fn pane_lines(app: &App, pane: Pane) -> Vec<Line<'static>> {
    match pane {
        Pane::Status => app.status_lines(),
        Pane::Files => app.file_lines(),
        Pane::Branches => mock::BRANCHES.iter().map(|s| theme::branch_line(s)).collect(),
        Pane::Commits => mock::COMMITS.iter().map(|s| theme::commit_line(s)).collect(),
        Pane::Stash => mock::STASH.iter().map(|s| Line::raw(*s)).collect(),
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

        let list = List::new(pane_lines(app, pane))
            .block(Block::bordered().title(title).border_style(border))
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));

        let mut state = ListState::default();
        let row_ct = app.row_count(pane);
        if row_ct > 0 {
            state.select(Some(app.selected(pane).min(row_ct - 1)));
        }

        frame.render_stateful_widget(list, rows[i], &mut state);
    }
}

fn draw_right_pane(frame: &mut Frame, app: &App, area: Rect) {
    let focused = Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD);

    let (title, body) = match app.focus {
        Pane::Status => (" Status ", mock::RIGHT_STATUS),
        Pane::Files => (" Diff ", mock::RIGHT_DIFF),
        Pane::Branches => (" Log ", mock::RIGHT_LOG),
        Pane::Commits => (" Commit ", mock::RIGHT_COMMIT),
        Pane::Stash => (" Stash ", mock::RIGHT_STASH),
    };

    let text: Text = match app.focus {
        Pane::Files | Pane::Branches | Pane::Commits => theme::diff_text(body),
        _ => body.into(),
    };

    let panel = Paragraph::new(text)
        .block(
            Block::bordered()
                .title(Line::styled(title, focused))
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
