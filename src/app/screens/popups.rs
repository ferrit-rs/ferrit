//! Popup rendering, separate from the screen and pane layout.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use crate::app::hints::HelpLine;
use crate::app::theme;
use crate::app::{CommandLogView, CommitPopupView, MenuView};
use crate::components::tui_overlay::anchor::Anchor;
use crate::components::tui_overlay::backdrop::Backdrop;
use crate::components::tui_overlay::overlay::Overlay;
use crate::components::tui_overlay::state::OverlayState;
use crate::components::ui::dialog::Dialog;
use crate::components::ui::key_bar::KeyBar;
use crate::components::ui::palette::Palette;
use crate::components::ui::panel::Panel;
use crate::components::ui::scroll_bar::ScrollBar;
use crate::components::ui::select_list::SelectList;

const CONFIRM_DIALOG_WIDTH_PERCENT: u16 = 70;
const CONFIRM_DIALOG_HEIGHT_PERCENT: u16 = 34;
const CONFIRM_DIALOG_TITLE: &str = " Confirm identity change? ";
const CONFIRM_DIALOG_HINT: &str = "Y: Confirm · N / Esc: Cancel";

/// The help screen: one line per binding of the focused pane and of the global
/// context, scrolled to `scroll`. Returns how many lines fit, so the scroll
/// keys know where the end is.
pub(super) fn draw_help(
    frame: &mut Frame<'_>,
    area: Rect,
    accent: ratatui::style::Color,
    lines: &[HelpLine],
    scroll: usize,
    palette: &Palette,
) -> usize {
    let width = 80.min(area.width);
    let height = area
        .height
        .min(u16::try_from(lines.len() + 3).unwrap_or(u16::MAX));
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(" keybindings ", focused))
        .size(width, height)
        .footer_rows(1)
        .border_style(focused)
        .render(frame, area);
    let rows = usize::from(dialog.body.height);
    let max = lines.len().saturating_sub(rows);
    let start = scroll.min(max);
    let key_width = lines
        .iter()
        .filter_map(|line| match line {
            HelpLine::Entry { keys, .. } => Some(keys.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        .min(24);
    let rendered: Vec<Line<'static>> = lines
        .iter()
        .skip(start)
        .take(rows)
        .map(|line| match line {
            HelpLine::Heading(text) => Line::styled(
                text.clone(),
                Style::new().fg(accent).add_modifier(Modifier::BOLD),
            ),
            HelpLine::Entry { keys, text } => Line::from(vec![
                Span::styled(
                    format!("{keys:<key_width$}  "),
                    Style::new().fg(palette.key),
                ),
                Span::raw(text.clone()),
            ]),
            HelpLine::Blank => Line::raw(""),
        })
        .collect();
    let [text_area, bar_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(dialog.body);
    frame.render_widget(Paragraph::new(rendered), text_area);
    ScrollBar::new(lines.len(), rows, start)
        .style(Style::new().fg(palette.idle))
        .render(frame, bar_area);
    let position = if max == 0 {
        String::new()
    } else {
        format!(" \u{b7} {}/{}", start + rows.min(lines.len()), lines.len())
    };
    frame.render_widget(
        Paragraph::new(Line::styled(
            format!("j/k scroll \u{b7} ? / Esc close{position}"),
            Style::new().fg(palette.idle),
        )),
        dialog.footer,
    );
    rows
}

/// Shared editor popup for commit messages and branch names.
pub(super) fn draw_commit(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &mut CommitPopupView<'_>,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let width = (area.width * 2 / 3).clamp(40.min(area.width), area.width);
    if let Some(description) = view.description {
        let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
        let idle = Style::new().fg(palette.idle);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(focused)
            .title(Line::styled(format!(" {} ", view.title), focused));
        // The commit editor always carries its overlay state; without it
        // there is nothing to anchor, so draw nothing.
        let Some(overlay_state) = view.overlay_state.take() else {
            return;
        };
        frame.render_stateful_widget(
            Overlay::new()
                .anchor(Anchor::Center)
                .width(Constraint::Percentage(72))
                .height(Constraint::Percentage(68))
                .backdrop(
                    Backdrop::new(ratatui::style::Color::Black).fg(ratatui::style::Color::DarkGray),
                )
                .block(block),
            area,
            overlay_state,
        );
        let Some(inner) = overlay_state.inner_area() else {
            return;
        };
        // The sign-off / no-verify line only exists when the toggles do (not
        // for a rebase reword, which runs the amend itself).
        let footer_rows = if view.toggles.is_some() { 2 } else { 1 };
        let [body_area, footer_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(footer_rows)]).areas(inner);
        let [summary_area, description_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).areas(body_area);
        let summary_style = if view.summary_focused { focused } else { idle };
        let description_style = if view.summary_focused { idle } else { focused };
        let summary_block = Panel::new()
            .title(Line::styled(" Summary ", summary_style))
            .bottom_title(theme::subject_counter(
                palette,
                view.input.text().chars().count(),
            ))
            .border_style(summary_style)
            .block();
        let summary_inner = summary_block.inner(summary_area);
        frame.render_widget(summary_block, summary_area);
        if view.summary_focused {
            view.input.render(frame, summary_inner);
        } else {
            view.input.render_inactive(frame, summary_inner);
        }

        let description_block = Panel::new()
            .title(Line::styled(" Description ", description_style))
            .border_style(description_style)
            .block();
        let description_inner = description_block.inner(description_area);
        frame.render_widget(description_block, description_area);
        if view.summary_focused {
            description.render_inactive(frame, description_inner);
        } else {
            description.render(frame, description_inner);
        }

        let hints = KeyBar::hints(view.hints, palette).line();
        let footer = match view.toggles {
            Some((sign_off, no_verify)) => {
                let on_off = |enabled: bool| if enabled { "on" } else { "off" };
                let status = Line::from(vec![
                    Span::styled("sign-off: ", Style::new().fg(palette.idle)),
                    Span::styled(on_off(sign_off), Style::new().fg(palette.add)),
                    Span::raw("   "),
                    Span::styled("no-verify: ", Style::new().fg(palette.idle)),
                    Span::styled(on_off(no_verify), Style::new().fg(palette.del)),
                ]);
                vec![status, hints]
            },
            None => vec![hints],
        };
        frame.render_widget(Paragraph::new(footer), footer_area);
        return;
    }

    let body_height = u16::try_from(view.input.lines().len().max(1)).unwrap_or(u16::MAX);
    let footer_height: u16 = if view.toggles.is_some() { 2 } else { 1 };
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(format!(" {} ", view.title), focused))
        .fit_content(width, body_height, footer_height)
        .border_style(focused)
        .render(frame, area);
    view.input.render(frame, dialog.body);

    let hints = KeyBar::hints(view.hints, palette).line();
    match view.toggles {
        Some((sign_off, no_verify)) => {
            let sign_off_span = if sign_off {
                Span::styled("on", Style::new().fg(palette.add))
            } else {
                Span::styled("off", Style::new().fg(palette.idle))
            };
            let verify_span = if no_verify {
                Span::styled(
                    "off",
                    Style::new().fg(palette.del).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("on", Style::new().fg(palette.add))
            };
            let status = Line::from(vec![
                Span::styled("sign-off: ", Style::new().fg(palette.idle)),
                sign_off_span,
                Span::raw("   "),
                Span::styled("verify: ", Style::new().fg(palette.idle)),
                verify_span,
            ]);
            frame.render_widget(Paragraph::new(vec![status, hints]), dialog.footer);
        },
        None => frame.render_widget(Paragraph::new(hints), dialog.footer),
    }
}

/// Confirm staging all worktree changes when `c` is pressed with an empty index.
pub(super) fn draw_commit_all_confirm(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut OverlayState,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(focused)
        .title(Line::styled(" Commit all files? ", focused));
    frame.render_stateful_widget(
        Overlay::new()
            .anchor(Anchor::Center)
            .width(Constraint::Percentage(68))
            .height(Constraint::Percentage(34))
            .backdrop(
                Backdrop::new(ratatui::style::Color::Black).fg(ratatui::style::Color::DarkGray),
            )
            .block(block),
        area,
        state,
    );
    let Some(inner) = state.inner_area() else {
        return;
    };
    let [heading, question, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(2),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new("No files staged")
            .style(Style::new().fg(palette.add).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center),
        heading,
    );
    frame.render_widget(
        Paragraph::new("You have not staged any files.\nCommit all files?")
            .alignment(Alignment::Center),
        question,
    );
    frame.render_widget(
        Paragraph::new(KeyBar::hints("Y: Yes, stage all | N / Esc: No", palette).line())
            .alignment(Alignment::Center),
        footer,
    );
}

pub(super) fn draw_confirmation(
    frame: &mut Frame<'_>,
    area: Rect,
    message: &str,
    state: &mut OverlayState,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(focused)
        .title(Line::styled(CONFIRM_DIALOG_TITLE, focused));
    frame.render_stateful_widget(
        Overlay::new()
            .anchor(Anchor::Center)
            .width(Constraint::Percentage(CONFIRM_DIALOG_WIDTH_PERCENT))
            .height(Constraint::Percentage(CONFIRM_DIALOG_HEIGHT_PERCENT))
            .backdrop(
                Backdrop::new(ratatui::style::Color::Black).fg(ratatui::style::Color::DarkGray),
            )
            .block(block),
        area,
        state,
    );
    let Some(inner) = state.inner_area() else {
        return;
    };
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    frame.render_widget(
        Paragraph::new(message)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Center),
        body,
    );
    frame.render_widget(
        Paragraph::new(KeyBar::hints(CONFIRM_DIALOG_HINT, palette).line())
            .alignment(Alignment::Center),
        footer,
    );
}

pub(super) fn draw_note(frame: &mut Frame<'_>, area: Rect, message: &str, palette: &Palette) {
    let width = 60.min(area.width);
    let content_lines = message.lines().count().max(1);
    let body_rows = u16::try_from(content_lines)
        .unwrap_or(u16::MAX)
        .saturating_add(1);
    let warn = Style::new().fg(palette.del).add_modifier(Modifier::BOLD);
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
            Style::new().fg(palette.idle),
        )),
        dialog.footer,
    );
}

/// The `@` viewer: every recorded command, newest at the bottom, scrolled up
/// by `view.from_bottom` rows (clamped to what exists).
pub(super) fn draw_command_log_view(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &CommandLogView,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(" command log ", focused))
        .size(
            (area.width * 4 / 5).max(40.min(area.width)),
            (area.height * 4 / 5).max(8.min(area.height)),
        )
        .footer_rows(1)
        .border_style(focused)
        .render(frame, area);
    let rows = usize::from(dialog.body.height);
    let total = view.records.len();
    let end = total.saturating_sub(view.from_bottom.min(total.saturating_sub(rows)));
    let start = end.saturating_sub(rows);
    let lines: Vec<Line<'static>> = view
        .records
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .map(|record| theme::command_line(palette, record))
        .collect();
    let body = if lines.is_empty() {
        vec![Line::styled(
            "no git command run yet",
            Style::new().fg(palette.idle),
        )]
    } else {
        lines
    };
    frame.render_widget(Paragraph::new(body), dialog.body);
    frame.render_widget(
        Paragraph::new(Line::styled(
            format!("{end}/{total} \u{b7} j/k scroll \u{b7} g/G oldest/newest \u{b7} Esc close"),
            Style::new().fg(palette.idle),
        )),
        dialog.footer,
    );
}

/// A list of actions with the highlighted row filled, `Enter` or a row's own
/// letter to run it.
pub(super) fn draw_menu(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &MenuView,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let rows = u16::try_from(view.rows.len()).unwrap_or(u16::MAX).max(1);
    let dialog = Dialog::new(Line::styled(format!(" {} ", view.title), focused))
        .fit_content(44.min(area.width), rows, 1)
        .border_style(focused)
        .render(frame, area);
    let lines: Vec<Line<'static>> = view
        .rows
        .iter()
        .map(|row| Line::from(format!(" {row}")))
        .collect();
    SelectList::new(&lines, view.selected)
        .selection_style(
            Style::new()
                .fg(palette.selection_fg)
                .bg(palette.selection)
                .add_modifier(Modifier::BOLD),
        )
        .render(frame, dialog.body);
    frame.render_widget(
        Paragraph::new(Line::styled(
            "Enter / letter run \u{b7} Esc close",
            Style::new().fg(palette.idle),
        )),
        dialog.footer,
    );
}
