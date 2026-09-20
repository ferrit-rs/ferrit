//! Popup rendering, separate from the screen and pane layout.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::CommitPopupView;
use crate::components::ui::dialog::Dialog;
use crate::components::ui::key_bar::KeyBar;
use crate::components::ui::select_list::SelectList;
use crate::{git, mock, theme};

pub(super) fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let width = 55.min(area.width);
    let body_rows = u16::try_from(mock::HELP.lines().count())
        .unwrap_or(area.height)
        .max(1);
    let focused = Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(" keybindings ", focused))
        .fit_content(width, body_rows, 0)
        .border_style(focused)
        .render(frame, area);
    frame.render_widget(Paragraph::new(mock::HELP), dialog.body);
}

/// Shared editor popup for commit messages and branch names.
pub(super) fn draw_commit(frame: &mut Frame<'_>, area: Rect, view: &CommitPopupView<'_>) {
    let width = (area.width * 2 / 3).clamp(40.min(area.width), area.width);
    let body_height = u16::try_from(view.input.lines().len().max(1)).unwrap_or(u16::MAX);
    let footer_height: u16 = if view.toggles.is_some() { 2 } else { 1 };
    let focused = Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(format!(" {} ", view.title), focused))
        .fit_content(width, body_height, footer_height)
        .border_style(focused)
        .render(frame, area);
    view.input.render(frame, dialog.body);

    let hints = KeyBar::hints(view.hints).line();
    match view.toggles {
        Some((sign_off, no_verify)) => {
            let sign_off_span = if sign_off {
                Span::styled("on", Style::new().fg(theme::ADD))
            } else {
                Span::styled("off", Style::new().fg(theme::IDLE))
            };
            let verify_span = if no_verify {
                Span::styled(
                    "off",
                    Style::new().fg(theme::DEL).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("on", Style::new().fg(theme::ADD))
            };
            let status = Line::from(vec![
                Span::styled("sign-off: ", Style::new().fg(theme::IDLE)),
                sign_off_span,
                Span::raw("   "),
                Span::styled("verify: ", Style::new().fg(theme::IDLE)),
                verify_span,
            ]);
            frame.render_widget(Paragraph::new(vec![status, hints]), dialog.footer);
        },
        None => frame.render_widget(Paragraph::new(hints), dialog.footer),
    }
}

pub(super) fn draw_remote_pick(
    frame: &mut Frame<'_>,
    area: Rect,
    remotes: &[git::remote::RemoteEntry],
    selected: usize,
) {
    let width = (area.width * 2 / 3).clamp(40.min(area.width), area.width);
    let body_height = u16::try_from(remotes.len().max(1)).unwrap_or(u16::MAX);
    let focused = Style::new().fg(theme::FOCUS).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(" Push to which remote? ", focused))
        .fit_content(width, body_height, 1)
        .border_style(focused)
        .render(frame, area);

    let lines: Vec<Line<'static>> = remotes
        .iter()
        .map(|remote| Line::raw(remote.name.clone()))
        .collect();
    SelectList::new(&lines, selected)
        .selection_style(theme::selection_style(true))
        .render(frame, dialog.body);
    KeyBar::hints("Push: Enter | Cancel: Esc").render(frame, dialog.footer);
}

pub(super) fn draw_note(frame: &mut Frame<'_>, area: Rect, message: &str) {
    let width = 60.min(area.width);
    let content_lines = message.lines().count().max(1);
    let body_rows = u16::try_from(content_lines)
        .unwrap_or(u16::MAX)
        .saturating_add(1);
    let warn = Style::new().fg(theme::DEL).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(" commit ", warn))
        .fit_content(width, body_rows, 1)
        .border_style(warn)
        .render(frame, area);
    frame.render_widget(
        Paragraph::new(message.to_owned()).wrap(Wrap { trim: false }),
        dialog.body,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            "Esc / Enter to dismiss",
            Style::new().fg(theme::IDLE),
        )),
        dialog.footer,
    );
}
