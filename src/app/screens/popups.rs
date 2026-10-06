//! Popup rendering, separate from the screen and pane layout.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use crate::app::create_remote::{ConfirmView, CreateRemoteView, Field, FormView};
use crate::app::hints::{self, HelpLine};
use crate::app::theme;
use crate::app::{CommandLogView, CommitPopupView, MenuView};
use crate::components::tui_overlay::anchor::Anchor;
use crate::components::tui_overlay::backdrop::Backdrop;
use crate::components::tui_overlay::overlay::Overlay;
use crate::components::tui_overlay::slide::Slide;
use crate::components::tui_overlay::state::OverlayState;
use crate::components::ui::dialog::Dialog;
use crate::components::ui::key_bar::KeyBar;
use crate::components::ui::palette::Palette;
use crate::components::ui::panel::Panel;
use crate::components::ui::scroll_bar::ScrollBar;
use crate::components::ui::select_list::SelectList;
use crate::components::ui::text_input::TextInput;
use crate::domain::git::host::Visibility;

/// The help screen: one line per binding of the focused pane and of the global
/// context, scrolled to `scroll`. Returns how many lines fit, so the scroll
/// keys know where the end is.
pub(super) fn draw_help(
    frame: &mut Frame<'_>,
    area: Rect,
    accent: ratatui::style::Color,
    lines: &[HelpLine],
    scroll: usize,
    overlay_state: &mut OverlayState,
    query: &TextInput,
    searching: bool,
    palette: &Palette,
) -> usize {
    let filtered = hints::filter_help_lines(lines, &query.text());
    let width = 80.min(area.width);
    let desired_height = u16::try_from(filtered.len().saturating_add(6)).unwrap_or(u16::MAX);
    let max_height = (area.height.saturating_mul(4) / 5).max(1);
    let height = desired_height.min(max_height).min(area.height);
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(focused)
        .title(Line::styled(" keybindings ", focused));
    if overlay_state.is_closed() {
        // Keep direct render-test assignment of `show_help = true` useful;
        // real input opens the persistent state before this function runs.
        overlay_state.open();
        overlay_state.tick(std::time::Duration::from_secs(1));
    }
    frame.render_stateful_widget(
        Overlay::new()
            .anchor(Anchor::Center)
            .slide(Slide::Bottom)
            .width(Constraint::Length(width))
            .height(Constraint::Length(height))
            .backdrop(
                Backdrop::new(ratatui::style::Color::Black).fg(ratatui::style::Color::DarkGray),
            )
            .bg(ratatui::style::Color::Reset)
            .block(block),
        area,
        overlay_state,
    );
    let Some(inner) = overlay_state.inner_area() else {
        return 1;
    };
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let [search_area, list_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(body);
    let search_style = if searching {
        focused
    } else {
        Style::new().fg(palette.idle)
    };
    let search_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(search_style)
        .style(Style::new().bg(palette.focus_box))
        .title(Line::styled(" SEARCH COMMAND ", search_style));
    let input_area = search_block.inner(search_area);
    frame.render_widget(search_block, search_area);
    let [label_area, value_area] =
        Layout::horizontal([Constraint::Length(9), Constraint::Min(0)]).areas(input_area);
    frame.render_widget(
        Paragraph::new(Line::styled("query: /", Style::new().fg(palette.key))),
        label_area,
    );
    if searching {
        query.render(frame, value_area);
    } else {
        query.render_inactive(frame, value_area);
    }

    let rows = usize::from(list_area.height);
    let max = filtered.len().saturating_sub(rows.max(1));
    let start = scroll.min(max);
    let key_width = filtered
        .iter()
        .filter_map(|line| match line {
            HelpLine::Entry { keys, .. } => Some(keys.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        .min(24);
    let rendered: Vec<Line<'static>> = filtered
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
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(list_area);
    let rendered = if rendered.is_empty() {
        vec![Line::styled(
            format!("no command matches {:?}", query.text()),
            Style::new().fg(palette.idle),
        )]
    } else {
        rendered
    };
    frame.render_widget(Paragraph::new(rendered), text_area);
    ScrollBar::new(filtered.len(), rows, start)
        .style(Style::new().fg(palette.idle))
        .render(frame, bar_area);
    let position = if max == 0 {
        String::new()
    } else {
        format!(
            " \u{b7} {}/{}",
            start + rows.min(filtered.len()),
            filtered.len()
        )
    };
    let footer_text = if searching {
        "type to filter \u{b7} enter apply \u{b7} esc cancel"
    } else {
        " / search \u{b7} j/k scroll \u{b7} ?/esc close"
    };
    frame.render_widget(
        Paragraph::new(Line::styled(
            format!("{footer_text}{position}"),
            Style::new().fg(palette.idle),
        )),
        footer,
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
        let footer_rows =
            if view.toggles.is_some() { 2 } else { 1 } + u16::from(view.author.is_some());
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
                let mut rows = vec![status];
                rows.extend(
                    view.author
                        .as_ref()
                        .map(|author| Line::styled(author.clone(), Style::new().fg(palette.idle))),
                );
                rows.push(hints);
                rows
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
    // Every record's command and its answer lines, one row each.
    let all: Vec<Line<'static>> = view
        .records
        .iter()
        .flat_map(|record| theme::command_lines(palette, record))
        .collect();
    let total = all.len();
    let end = total.saturating_sub(view.from_bottom.min(total.saturating_sub(rows)));
    let start = end.saturating_sub(rows);
    let lines: Vec<Line<'static>> = all.get(start..end).unwrap_or_default().to_vec();
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
    let hint_width = u16::try_from(view.hint.chars().count() + 4).unwrap_or(u16::MAX);
    let dialog = Dialog::new(Line::styled(format!(" {} ", view.title), focused))
        .fit_content(44.max(hint_width).min(area.width), rows, 1)
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
            if view.hint.is_empty() {
                "Enter / letter run \u{b7} Esc close"
            } else {
                view.hint
            },
            Style::new().fg(palette.idle),
        )),
        dialog.footer,
    );
}

/// The popups of creating the GitHub repository: the `gh` check, the form, and
/// the last question (`docs/PLAN_15_CREATE_REMOTE.md`).
pub(super) fn draw_create_remote(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &CreateRemoteView<'_>,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    match view {
        CreateRemoteView::Checking => {
            let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
            let dialog = Dialog::new(Line::styled(" Create on GitHub ", focused))
                .fit_content(44.min(area.width), 1, 1)
                .border_style(focused)
                .render(frame, area);
            frame.render_widget(
                Paragraph::new(Line::styled(
                    " checking gh\u{2026}",
                    Style::new().fg(palette.idle),
                )),
                dialog.body,
            );
            frame.render_widget(
                Paragraph::new(KeyBar::hints("Cancel: Esc", palette).line()),
                dialog.footer,
            );
        },
        CreateRemoteView::Form(form) => draw_create_form(frame, area, form, accent, palette),
        CreateRemoteView::Confirm(confirm) => {
            draw_create_confirm(frame, area, confirm, accent, palette);
        },
    }
}

/// A framed text field like the commit popup's: the title on the border, a
/// character counter on the bottom border, and the text wrapped over the rows
/// of the box, so what was typed is always in view.
fn draw_text_box(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    input: &TextInput,
    focused: bool,
    max: usize,
    palette_style: (Style, Style),
) {
    let style = if focused {
        palette_style.0
    } else {
        palette_style.1
    };
    let counter = Line::styled(format!(" {}/{max} ", input.text().chars().count()), style);
    let block = Panel::new()
        .title(Line::styled(format!(" {title} "), style))
        .bottom_title(counter.right_aligned())
        .border_style(style)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if focused {
        input.render(frame, inner);
    } else {
        input.render_inactive(frame, inner);
    }
}

fn draw_create_form(
    frame: &mut Frame<'_>,
    area: Rect,
    form: &FormView<'_>,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let idle = Style::new().fg(palette.idle);
    // The name (two rows: up to 100 characters), the visibility, the
    // description (up to 350 characters wrap over its rows), an error line.
    let dialog = Dialog::new(Line::styled(" Create on GitHub ", focused))
        .fit_content(70.min(area.width), 14, 1)
        .border_style(focused)
        .render(frame, area);
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(dialog.body);
    let at = |i: usize| rows.get(i).copied().unwrap_or_default();

    draw_text_box(
        frame,
        at(0),
        "Name",
        form.name,
        form.focus == Field::Name,
        100,
        (focused, idle),
    );

    let radio = |on: bool| if on { "(\u{2022})" } else { "( )" };
    let label_style = if form.focus == Field::Visibility {
        focused
    } else {
        idle
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Visibility   ", label_style),
            Span::raw(format!(
                "{} private   {} public",
                radio(form.visibility == Visibility::Private),
                radio(form.visibility == Visibility::Public)
            )),
        ])),
        at(1),
    );

    draw_text_box(
        frame,
        at(2),
        "Description",
        form.description,
        form.focus == Field::Description,
        350,
        (focused, idle),
    );

    if let Some(error) = form.error {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {error}"),
                Style::new().fg(palette.del),
            )),
            at(3),
        );
    }
    frame.render_widget(
        Paragraph::new(KeyBar::hints("Next: Tab   Continue: Enter   Cancel: Esc", palette).line()),
        dialog.footer,
    );
}

fn draw_create_confirm(
    frame: &mut Frame<'_>,
    area: Rect,
    confirm: &ConfirmView,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    // A public repository takes the colour of the discard prompt.
    let border = match confirm.visibility {
        Visibility::Private => Style::new().fg(accent).add_modifier(Modifier::BOLD),
        Visibility::Public => Style::new().fg(palette.del).add_modifier(Modifier::BOLD),
    };
    let rows = u16::try_from(confirm.lines.len())
        .unwrap_or(u16::MAX)
        .max(1);
    let hint_width = u16::try_from(confirm.hint.chars().count() + 4).unwrap_or(u16::MAX);
    let title_width = u16::try_from(confirm.title.chars().count() + 6).unwrap_or(u16::MAX);
    // The widest line shows whole (a long SSH host, a long branch name).
    let text_width = confirm
        .lines
        .iter()
        .map(|line| u16::try_from(line.chars().count() + 4).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(0);
    let dialog = Dialog::new(Line::styled(format!(" {} ", confirm.title), border))
        .fit_content(
            48.max(hint_width)
                .max(title_width)
                .max(text_width)
                .min(area.width),
            rows,
            1,
        )
        .border_style(border)
        .render(frame, area);
    let lines: Vec<Line<'static>> = confirm
        .lines
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == 0 { border } else { Style::new() };
            Line::styled(format!(" {text}"), style)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), dialog.body);
    frame.render_widget(
        Paragraph::new(Line::styled(confirm.hint, Style::new().fg(palette.idle))),
        dialog.footer,
    );
}
