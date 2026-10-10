//! The right column: `draw`.

use crate::ui::components::diff::views::DiffView;
use crate::ui::components::panes::nav::Pane;
use crate::ui::components::welcome::welcome_lines;
use crate::ui::draw::{Landed, RenderState};
use crate::ui::image::preview::Preview;
use crate::ui::scene::Scene;
use crate::ui::{mock, row_lines};
use ferrit_domain::diff::DiffSide;
use ferrit_tui::theme::palette::Palette;
use ferrit_tui::widgets::chrome::lists::ScrollBar;
use ferrit_tui::widgets::chrome::panel::Panel;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui_image::{Resize, StatefulImage};
use std::ops::Range;

pub(crate) fn draw_files_columns(
    frame: &mut Frame<'_>,
    app: &Scene<'_>,
    landed: &mut Landed,
    area: Rect,
) {
    landed.right_area = Some(area);
    let palette = app.palette();

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
    let unstaged_cursor = diff_cursor_for(cursor.clone(), DiffSide::Worktree);
    let staged_cursor = diff_cursor_for(cursor, DiffSide::Staged);

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
        &palette,
    );
    let viewport = draw_diff_column(
        frame,
        right,
        &diff_column_title(" Staged Changes ", staged_cursor.is_some(), hint.as_deref()),
        &staged,
        scroll,
        staged_cursor,
        &palette,
    );
    landed.right_viewport = Some(viewport);
}

/// One-sided file changes use a single full-width panel, matching lazygit's
/// default `gui.splitDiff: auto` behavior. Pick staged when no worktree diff.
pub(crate) fn draw_single_file_diff(
    frame: &mut Frame<'_>,
    app: &Scene<'_>,
    landed: &mut Landed,
    area: Rect,
) {
    landed.right_area = Some(area);
    let palette = app.palette();
    let DiffView::Files(files) = app.diff_view() else {
        return;
    };
    let (side, diff, title) = if files.unstaged.text.trim().is_empty() {
        (DiffSide::Staged, files.staged.clone(), " Staged Changes ")
    } else {
        (
            DiffSide::Worktree,
            files.unstaged.clone(),
            " Unstaged Changes ",
        )
    };
    let cursor = diff_cursor_for(app.diff_cursor(), side);
    let scroll = app.right_scroll();
    let block = Panel::new()
        .title(Line::styled(title, Style::new().fg(palette.idle)))
        .border_style(Style::new().fg(palette.idle))
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stat_row, diff_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    frame.render_widget(
        Paragraph::new(row_lines::status::stat_line(&palette, diff.stat())),
        stat_row,
    );
    let mut text = diff.delta_output(diff_area.width as usize).map_or_else(
        || row_lines::diff::render_diff(&palette, &diff, None, diff_area.width as usize),
        |formatted| row_lines::diff::render_delta(&formatted, diff_area.width as usize),
    );
    overlay_diff_cursor(&mut text, cursor, diff_area.width as usize, &palette);
    let total = text.lines.len();
    frame.render_widget(
        Paragraph::new(text).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        diff_area,
    );
    landed.right_viewport = Some(diff_area.height as usize);
    ScrollBar::new(total, diff_area.height as usize, scroll).render(frame, diff_area);
}

/// `app.diff_cursor()`'s `(line, V-select range)` for `side`, or `None` when
/// the cursor is on the other side (or `Mode::Diff` isn't up at all).
pub(crate) fn diff_cursor_for(
    cursor: Option<(DiffSide, usize, Option<Range<usize>>)>,
    side: DiffSide,
) -> Option<(usize, Option<Range<usize>>)> {
    let (cursor_side, line, selection) = cursor?;
    (cursor_side == side).then_some((line, selection))
}

/// A Files-split column title, with the `Mode::Diff` granule hint appended
/// (`hunk 1/3` / `lines 41-42`) when `active` — the cursor's own column.
pub(crate) fn diff_column_title(base: &str, active: bool, hint: Option<&str>) -> String {
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
/// same whether the body came from `row_lines::diff::render_diff` or from delta.
pub(crate) fn draw_diff_column(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    diff: &ferrit_domain::diff::Diff,
    scroll: usize,
    cursor: Option<(usize, Option<Range<usize>>)>,
    palette: &Palette,
) -> usize {
    let block = Panel::new()
        .title(Line::styled(
            title.to_owned(),
            Style::new().fg(palette.idle),
        ))
        .border_style(Style::new().fg(palette.idle))
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stat_row, diff_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    frame.render_widget(
        Paragraph::new(row_lines::status::stat_line(palette, diff.stat())),
        stat_row,
    );

    let mut text = diff.delta_output(diff_area.width as usize).map_or_else(
        || row_lines::diff::render_diff(palette, diff, None, diff_area.width as usize),
        |formatted| row_lines::diff::render_delta(&formatted, diff_area.width as usize),
    );
    overlay_diff_cursor(&mut text, cursor, diff_area.width as usize, palette);
    let total = text.lines.len();
    let panel = Paragraph::new(text).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(panel, diff_area);

    let viewport = diff_area.height as usize;
    ScrollBar::new(total, viewport, scroll).render(frame, diff_area);
    viewport
}

/// Paint the `Mode::Diff` cursor onto an already-rendered diff body: a
/// full-width reversed bar on the cursor line, and the palette's selection colour
/// background across a V-selection. Applied after rendering, not woven into
/// `row_lines::diff::render_diff`, so it works identically over that native path and
/// over delta's ANSI-derived one.
pub(crate) fn overlay_diff_cursor(
    text: &mut Text<'static>,
    cursor: Option<(usize, Option<Range<usize>>)>,
    width: usize,
    palette: &Palette,
) {
    let Some((line, selection)) = cursor else {
        return;
    };
    if let Some(range) = selection {
        for i in range {
            if let Some(l) = text.lines.get_mut(i) {
                pad_line(l, width);
                for span in &mut l.spans {
                    span.style = span.style.bg(palette.selection);
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
pub(crate) fn pad_line(line: &mut Line<'static>, width: usize) {
    let padding = width.saturating_sub(line.width());
    if padding > 0 {
        line.spans.push(Span::raw(" ".repeat(padding)));
    }
}

pub(crate) fn draw_right_pane(
    frame: &mut Frame<'_>,
    app: &Scene<'_>,
    render: &mut RenderState,
    landed: &mut Landed,
    area: Rect,
) {
    let palette = &app.palette();
    // Remembered for mouse-wheel routing: a wheel event over this rect scrolls
    // the diff, one over the left column moves the selection.
    landed.right_area = Some(area);

    let focused = Style::new()
        .fg(app.theme.config.color())
        .add_modifier(Modifier::BOLD);
    let idle = Style::new().fg(palette.idle);
    // Branches normally previews nothing (" Log "); once drilled into a
    // branch's commit list, a selected row shows a real diff, so the title
    // matches what the Commits pane calls the same view: " Patch ".
    let right_title = if app.nav.focus == Pane::Branches
        && matches!(app.diff_view(), DiffView::Commit(..))
    {
        " Patch "
    } else if app.nav.focus == Pane::Files && !app.is_mock() && app.row_count(Pane::Files) == 0 {
        // Nothing changed: lazygit's "Diff" pane says so, instead of keeping
        // the "Unstaged changes" title over an empty box.
        " Diff "
    } else {
        app.nav.focus.right_title()
    };
    let border = if app.right_focused() { focused } else { idle };

    // An image selection takes over the right pane; otherwise it is mock text.
    match &mut render.preview {
        Preview::Image(proto) => {
            // Same shape as `render_resized_image` in the ratatui-image demo:
            // draw the border, then hand `StatefulImage` the inner area and a
            // `&mut StatefulProtocol` so it resizes + re-encodes to fit.
            let block = Panel::new()
                .title(Line::styled(" Preview ", focused))
                .border_style(border)
                .block();
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
                    Panel::new()
                        .title(Line::styled(right_title, focused))
                        .border_style(border)
                        .block(),
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

    let block = Panel::new()
        .title(Line::styled(right_title, focused))
        .border_style(border)
        .block();

    // Real `git show` output (a commit, or a drilled branch's commit): git-
    // native colouring, vertical scroll from `app.right_scroll()`, a reverse-
    // highlight on the file header a `]` / `[` jump last landed on, and a
    // scrollbar when it overflows. A Files selection never reaches here: it
    // gets its own two-column split (`draw_files_columns`) before this
    // function is even called.
    let scroll = app.right_scroll();
    if let DiffView::Commit(_, diff) | DiffView::Stash(_, diff) = app.diff_view() {
        let diff_area = block.inner(area);
        let anchors = diff.file_lines();
        let raw_total = diff.text.lines().count();
        let focus = anchors.iter().position(|&l| l == scroll).map(|i| {
            let end = anchors.get(i + 1).copied().unwrap_or(raw_total);
            scroll..end
        });
        let Some((text, total, _stat)) = app.right.rendered_diff(
            &app.prefs.palette,
            &mut render.diff_cache,
            focus.as_ref(),
            diff_area.width as usize,
        ) else {
            return;
        };
        frame.render_widget(block, area);

        let raw_max = raw_total.saturating_sub(diff_area.height as usize);
        let display_max = total.saturating_sub(diff_area.height as usize);
        let display_scroll = if raw_max == 0 {
            0
        } else {
            scroll
                .min(raw_max)
                .saturating_mul(display_max)
                .checked_div(raw_max)
                .unwrap_or_default()
        };
        let panel =
            Paragraph::new(text).scroll((u16::try_from(display_scroll).unwrap_or(u16::MAX), 0));
        frame.render_widget(panel, diff_area);
        let viewport = diff_area.height as usize;
        ScrollBar::new(total, viewport, display_scroll).render(frame, diff_area);
        landed.right_viewport = Some(diff_area.height as usize);
        return;
    }

    if let DiffView::Note(msg) = app.diff_view() {
        let panel = Paragraph::new(Line::styled(
            msg.clone(),
            Style::new().fg(palette.idle).add_modifier(Modifier::DIM),
        ))
        .block(block)
        .wrap(Wrap { trim: false });
        frame.render_widget(panel, area);
        return;
    }

    // Branches focused, not drilled in: the selected branch's own commits,
    // shown passively (lazygit's live branch -> log preview, no Enter
    // needed) as multi-line `git log`-style blocks (`row_lines::rows::branch_log_block`)
    // rather than the compact one-line rows the Commits pane uses — there is
    // a whole pane's width to spend here. No gutter/stat/hunk-jump, that
    // treatment is for an actual diff once Enter drills into a specific
    // commit, but it does scroll like one (`right_is_diff`), so J/K,
    // PageUp/Down and the wheel move this list instead of leaking through to
    // the Branches selection.
    if let DiffView::BranchLog(log) = app.diff_view() {
        let inner = block.inner(area);
        let lines: Vec<Line<'static>> = if log.commits.is_empty() {
            vec![Line::raw("no commits yet")]
        } else {
            log.commits
                .iter()
                .flat_map(|commit| row_lines::rows::branch_log_block(palette, commit))
                .collect()
        };
        let total = lines.len();
        let scroll = app.right_scroll();
        frame.render_widget(block, area);
        let panel = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0));
        frame.render_widget(panel, inner);
        let viewport = inner.height as usize;
        ScrollBar::new(total, viewport, scroll).render(frame, inner);
        landed.right_viewport = Some(viewport);
        return;
    }

    // Status: lazygit's welcome screen, not repo data — same in mock and on
    // a real repo, so this comes before the mock/real split below.
    if app.nav.focus == Pane::Status {
        let panel = Paragraph::new(welcome_lines(
            area.width,
            area.height,
            app.theme.config.color(),
            palette,
        ))
        .block(block)
        .wrap(Wrap { trim: false });
        frame.render_widget(panel, area);
        return;
    }

    // `App::mock()`: the sample text. A real repo with nothing selected (no
    // files, no commits) just leaves the pane blank.
    if !app.is_mock() {
        let empty_files = app.nav.focus == Pane::Files && app.row_count(Pane::Files) == 0;
        let empty_stash = app.nav.focus == Pane::Stash && app.row_count(Pane::Stash) == 0;
        let text = if empty_files {
            "No changed files"
        } else if empty_stash {
            "No stash entries"
        } else {
            ""
        };
        frame.render_widget(Paragraph::new(text).block(block), area);
        return;
    }

    // Status already returned above (the welcome screen shows in mock too).
    // Branches has no mock body either: `App::mock()` has no repo, so there
    // is nothing to preview or drill into (G7); the mock path matches that
    // by leaving it blank rather than showing a fake sample.
    let body = match app.nav.focus {
        Pane::Status | Pane::Branches => "",
        Pane::Files => mock::RIGHT_DIFF,
        Pane::Commits => mock::RIGHT_COMMIT,
        Pane::Stash => mock::RIGHT_STASH,
    };

    let text: Text<'_> = match app.nav.focus {
        Pane::Files | Pane::Commits => row_lines::diff::diff_lines(palette, body, None),
        _ => body.into(),
    };

    let panel = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}
