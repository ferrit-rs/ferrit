//! Rendering for the two-sided working-tree diff view.

use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::app::{App, DiffView};
use crate::components::ui::panel::Panel;
use crate::components::ui::scroll_bar::ScrollBar;
use crate::{git, theme};

pub(super) fn draw_files_columns(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    app.set_right_area(area);

    let DiffView::Files(files) = app.diff_view() else {
        return;
    };
    let unstaged = files.unstaged.clone();
    let staged = files.staged.clone();

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);

    let scroll = app.right_scroll();
    let cursor = app.diff_cursor();
    let hint = app.diff_granule_hint();
    let unstaged_cursor = diff_cursor_for(cursor.clone(), git::diff::DiffSide::Worktree);
    let staged_cursor = diff_cursor_for(cursor, git::diff::DiffSide::Staged);

    draw_diff_column(
        frame,
        left,
        &diff_column_title(
            " Unstaged Changes ",
            unstaged_cursor.is_some(),
            hint.as_deref(),
        ),
        &unstaged,
        scroll,
        unstaged_cursor,
    );
    let viewport = draw_diff_column(
        frame,
        right,
        &diff_column_title(" Staged Changes ", staged_cursor.is_some(), hint.as_deref()),
        &staged,
        scroll,
        staged_cursor,
    );
    app.set_right_viewport(viewport);
}

/// One-sided file changes use a single full-width panel, matching LazyGit's
/// default `gui.splitDiff: auto` behavior. Pick staged when no worktree diff.
pub(super) fn draw_single_file_diff(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    app.set_right_area(area);
    let DiffView::Files(files) = app.diff_view() else {
        return;
    };
    let (side, diff, title) = if files.unstaged.text.trim().is_empty() {
        (
            git::diff::DiffSide::Staged,
            files.staged.clone(),
            " Staged Changes ",
        )
    } else {
        (
            git::diff::DiffSide::Worktree,
            files.unstaged.clone(),
            " Unstaged Changes ",
        )
    };
    let cursor = diff_cursor_for(app.diff_cursor(), side);
    let scroll = app.right_scroll();
    let block = Panel::new()
        .title(Line::styled(title, Style::new().fg(theme::IDLE)))
        .border_style(Style::new().fg(theme::IDLE))
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stat_row, diff_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    frame.render_widget(Paragraph::new(theme::stat_line(diff.stat())), stat_row);
    let mut text = diff.delta_output(diff_area.width as usize).map_or_else(
        || theme::render_diff(&diff, None, diff_area.width as usize),
        |formatted| theme::render_delta(&formatted, diff_area.width as usize),
    );
    overlay_diff_cursor(&mut text, cursor, diff_area.width as usize);
    let total = text.lines.len();
    frame.render_widget(
        Paragraph::new(text).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        diff_area,
    );
    app.set_right_viewport(diff_area.height as usize);
    ScrollBar::new(total, diff_area.height as usize, scroll).render(frame, diff_area);
}

/// `app.diff_cursor()`'s `(line, V-select range)` for `side`, or `None` when
/// the cursor is on the other side (or `Mode::Diff` isn't up at all).
fn diff_cursor_for(
    cursor: Option<(git::diff::DiffSide, usize, Option<Range<usize>>)>,
    side: git::diff::DiffSide,
) -> Option<(usize, Option<Range<usize>>)> {
    let (cursor_side, line, selection) = cursor?;
    (cursor_side == side).then_some((line, selection))
}

/// A Files-split column title, with the `Mode::Diff` granule hint appended
/// (`hunk 1/3` / `lines 41-42`) when `active` — the cursor's own column.
fn diff_column_title(base: &str, active: bool, hint: Option<&str>) -> String {
    match (active, hint) {
        (true, Some(hint)) => format!("{} ({hint}) ", base.trim_end()),
        _ => base.to_owned(),
    }
}

/// One column of the Files split (`draw_files_columns`): border, stat line,
/// diff body scrolled to the shared `scroll` line, and a scrollbar when it
/// overflows. Returns the diff body's own height for the caller's shared
/// viewport. An empty side (nothing staged, or nothing left unstaged) just
/// shows a `0 files changed` stat and a blank body — the common half-staged
/// case is the one this exists for, not worth a special case.
///
/// `cursor`, when this column is the `Mode::Diff` cursor's own side, is
/// `(cursor line, V-select range)` — both indices into `diff`'s own lines,
/// applied as a post-render overlay (`overlay_diff_cursor`) so it works the
/// same whether the body came from `theme::render_diff` or from delta.
fn draw_diff_column(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    diff: &git::diff::Diff,
    scroll: usize,
    cursor: Option<(usize, Option<Range<usize>>)>,
) -> usize {
    let block = Panel::new()
        .title(Line::styled(title.to_owned(), Style::new().fg(theme::IDLE)))
        .border_style(Style::new().fg(theme::IDLE))
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stat_row, diff_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    frame.render_widget(Paragraph::new(theme::stat_line(diff.stat())), stat_row);

    let mut text = diff.delta_output(diff_area.width as usize).map_or_else(
        || theme::render_diff(diff, None, diff_area.width as usize),
        |formatted| theme::render_delta(&formatted, diff_area.width as usize),
    );
    overlay_diff_cursor(&mut text, cursor, diff_area.width as usize);
    let total = text.lines.len();
    let panel = Paragraph::new(text).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(panel, diff_area);

    let viewport = diff_area.height as usize;
    ScrollBar::new(total, viewport, scroll).render(frame, diff_area);
    viewport
}

/// Paint the `Mode::Diff` cursor onto an already-rendered diff body: a
/// full-width reversed bar on the cursor line, and `theme::SELECTION`'s blue
/// background across a V-selection. Applied after rendering, not woven into
/// `theme::render_diff`, so it works identically over that native path and
/// over delta's ANSI-derived one.
fn overlay_diff_cursor(
    text: &mut Text<'static>,
    cursor: Option<(usize, Option<Range<usize>>)>,
    width: usize,
) {
    let Some((line, selection)) = cursor else {
        return;
    };
    if let Some(range) = selection {
        for i in range {
            if let Some(l) = text.lines.get_mut(i) {
                pad_line(l, width);
                for span in &mut l.spans {
                    span.style = span.style.bg(theme::SELECTION);
                }
            }
        }
    }
    if let Some(l) = text.lines.get_mut(line) {
        pad_line(l, width);
        for span in &mut l.spans {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }
}

/// Pad `line` to `width` with a blank trailing span so a full-line
/// background/reverse overlay covers the whole row, not just its text.
fn pad_line(line: &mut Line<'static>, width: usize) {
    let padding = width.saturating_sub(line.width());
    if padding > 0 {
        line.spans.push(Span::raw(" ".repeat(padding)));
    }
}
