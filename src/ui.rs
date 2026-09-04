//! `draw(frame, &app)`: the whole screen, top-level layout down to widgets.
//!
//! Pure rendering. It reads `App` and `mock`, never mutates, never touches a
//! terminal, so `tests/render.rs` can call it against a `TestBackend`.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, List, ListState, Paragraph, Wrap};

use crate::app::{App, Pane, PANES};
use crate::mock;

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
            Style::new().yellow().bold()
        } else {
            Style::new().dark_gray()
        };

        let items = pane.items();
        let list = List::new(items.iter().copied())
            .block(
                Block::bordered()
                    .title(format!(" {} ", pane.title()))
                    .border_style(border),
            )
            .highlight_style(Style::new().reversed());

        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(app.selected(pane)));
        }

        frame.render_stateful_widget(list, rows[i], &mut state);
    }
}

fn draw_right_pane(frame: &mut Frame, app: &App, area: Rect) {
    let (title, body) = match app.focus {
        Pane::Status => (" Status ", mock::RIGHT_STATUS),
        Pane::Files => (" Diff ", mock::RIGHT_DIFF),
        Pane::Branches => (" Log ", mock::RIGHT_LOG),
        Pane::Commits => (" Commit ", mock::RIGHT_COMMIT),
        Pane::Stash => (" Stash ", mock::RIGHT_STASH),
    };

    let panel = Paragraph::new(body)
        .block(Block::bordered().title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

fn draw_command_log(frame: &mut Frame, area: Rect) {
    let lines: Vec<Line> = mock::COMMAND_LOG.iter().map(|s| Line::raw(*s)).collect();
    let panel = Paragraph::new(lines).block(Block::bordered().title(" command log "));
    frame.render_widget(panel, area);
}

fn draw_keybar(frame: &mut Frame, area: Rect) {
    let bar = Paragraph::new(Line::raw(mock::KEYBAR).style(Style::new().dark_gray()));
    frame.render_widget(bar, area);
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
            .title(" keybindings ")
            .border_style(Style::new().yellow().bold()),
    );

    frame.render_widget(Clear, rect);
    frame.render_widget(overlay, rect);
}
