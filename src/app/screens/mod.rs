//! `draw(frame, &app)`: the whole screen, top-level layout down to widgets.
//!
//! Pure rendering. It reads `App` and `mock`, never mutates, never touches a
//! terminal, so `tests/render.rs` can call it against a `TestBackend`.
//! Colours come from `theme`.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui_image::{Resize, StatefulImage};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, DiffView, PANES, Pane, PopupView};
use crate::app::{mock, theme};
use crate::components::ui::key_bar::KeyBar;
use crate::components::ui::pane_list::PaneList;
use crate::components::ui::panel::Panel;
use crate::components::ui::scroll_bar::ScrollBar;
use crate::domain::git::command_log;
use crate::domain::image::preview::Preview;

mod diff;
mod popups;
pub(super) mod profile;

/// Render the full screen for the current `App` state.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();

    let [content, log, keybar] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .areas(area);

    // LazyGit splits the diff only for partially staged files. One-sided
    // changes use the full-width diff pane.
    let files_split = app.focus == Pane::Files
        && matches!(app.diff_view(), DiffView::Files(files)
            if !files.unstaged.text.trim().is_empty() && !files.staged.text.trim().is_empty());

    // lazygit's default `sidePanelWidth: 0.3333`: the left column takes a third
    // of the width, floored so it stays usable on a narrow terminal.
    let side = if files_split {
        (area.width / 8).max(14)
    } else {
        (area.width / 3).max(24)
    };
    let [left, right] =
        Layout::horizontal([Constraint::Length(side), Constraint::Min(0)]).areas(content);

    let show_help = app.show_help;
    draw_left_column(frame, app, left);
    if files_split {
        diff::draw_files_columns(frame, app, right);
    } else if app.focus == Pane::Files && matches!(app.diff_view(), DiffView::Files(_)) {
        diff::draw_single_file_diff(frame, app, right);
    } else {
        draw_right_pane(frame, app, right);
    }
    draw_command_log(frame, app, log);
    draw_keybar(frame, keybar, app);

    if app.author_overlay.is_closed() {
        app.profile_hit_areas = profile::ProfileHitAreas::default();
    }

    if !app.author_overlay.is_closed() {
        let profile_data = app.profile().clone();
        let theme_view = profile::ThemeView {
            config: &app.theme_config,
            mode: app.theme_mode,
            rgb_channel: app.theme_rgb_channel,
            palette_selected: app.theme_palette_selected,
            picker_display: app.theme_picker_display,
            dirty: app.theme_config != app.theme_saved_config,
        };
        app.profile_hit_areas = profile::draw_author(
            frame,
            area,
            &mut app.author_overlay,
            &profile_data,
            &mut app.profile_scroll,
            &theme_view,
            app.selected_author.as_ref(),
        );
    }

    if show_help {
        popups::draw_help(frame, area, app.theme_config.color());
    }
    let accent = app.theme_config.color();
    match app.popup_view() {
        Some(
            PopupView::Commit(mut view)
            | PopupView::NewBranch(mut view)
            | PopupView::Stash(mut view)
            | PopupView::Upstream(mut view),
        ) => {
            popups::draw_commit(frame, area, &mut view, accent);
        },
        Some(PopupView::CommitAllConfirm(state)) => {
            popups::draw_commit_all_confirm(frame, area, state, accent);
        },
        Some(PopupView::Note(message)) => popups::draw_note(frame, area, message),
        Some(PopupView::Menu(view)) => popups::draw_menu(frame, area, &view, accent),
        Some(PopupView::CommandLog(view)) => {
            popups::draw_command_log_view(frame, area, &view, accent);
        },
        None => {},
    }
    if let Some(message) = app.confirm_dialog_message().map(str::to_owned) {
        popups::draw_confirmation(
            frame,
            area,
            &message,
            &mut app.confirm_overlay,
            app.theme_config.color(),
        );
    }

    if let Some(toast) = &mut app.toast {
        toast.render(frame, area);
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

fn draw_left_column(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
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
    let focus_index = DYNAMIC.iter().position(|&p| p == app.focus);

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
        app.set_left_area(pane, row);

        let focused = app.focus == pane;
        let border = if focused {
            Style::new()
                .fg(app.theme_config.color())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme::IDLE)
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
                    .fg(app.theme_config.color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(theme::IDLE)
            },
        );

        let mut panel = Panel::new().title(title).border_style(border);
        if let Some((cur, total)) = app.counter(pane) {
            panel = panel.bottom_title(theme::counter_line(cur, total));
        }
        let block = panel.block();

        let row_ct = app.row_count(pane);
        let lines = pane_lines(app, pane);
        let offset = PaneList::new(lines, block)
            .selected((row_ct > 0).then(|| app.selected(pane).min(row_ct - 1)))
            .offset(app.list_offset(pane))
            .highlight_style(theme::selection_style(focused))
            .scrollbar_style(border)
            .render(frame, row);
        // Ratatui may have moved the offset to keep the selection on screen;
        // copy it back so a click in a scrolled list maps to the right row.
        app.set_list_offset(pane, offset);
    }
}

/// Three `ferrit` wordmarks, generated with `toilet` rather than
/// hand-drawn, so the glyphs are guaranteed to line up — lazygit grows its
/// own banner as the terminal grows rather than showing one fixed size, so
/// `welcome_lines` picks the biggest of these three that still fits instead
/// of the single fixed logo the first attempt at this used. Each row is
/// trimmed of the trailing blank columns `toilet` pads it to (avoids
/// trailing whitespace in the source); `welcome_lines` re-pads every row to
/// its tier's width before centering it, since figlet-style fonts rely on
/// every row spanning the same width — letting each row's own (different)
/// trimmed length drive `Line::centered()` would shift rows against each
/// other and break the letterforms.
struct Wordmark {
    art: &'static str,
    width: usize,
    /// Right-pane `(width, height)` needed to show this tier without
    /// clipping or crowding the text below it.
    min_area: (u16, u16),
}

/// `toilet -f smmono12 ferrit`.
const WORDMARK_SMALL: Wordmark = Wordmark {
    art: "\
  ▄▄                  █
 ▐▛▀                  ▀   ▐▌
▐███  ▟█▙  █▟█▌ █▟█▌ ██  ▐███
 ▐▌  ▐▙▄▟▌ █▘   █▘    █   ▐▌
 ▐▌  ▐▛▀▀▘ █    █     █   ▐▌
 ▐▌  ▝█▄▄▌ █    █   ▗▄█▄▖ ▐▙▄
 ▝▘   ▝▀▀  ▀    ▀   ▝▀▀▀▘  ▀▀",
    width: 30,
    min_area: (40, 16),
};
/// `toilet -f mono12 ferrit`. Same width as `WORDMARK_LARGE` (both are a
/// 60-column canvas) — what changes between the two is height, not width.
const WORDMARK_MEDIUM: Wordmark = Wordmark {
    art: "\
    ▄▄▄▄                                    ██
   ██▀▀▀                                    ▀▀       ██
 ███████    ▄████▄    ██▄████   ██▄████   ████     ███████
   ██      ██▄▄▄▄██   ██▀       ██▀         ██       ██
   ██      ██▀▀▀▀▀▀   ██        ██          ██       ██
   ██      ▀██▄▄▄▄█   ██        ██       ▄▄▄██▄▄▄    ██▄▄▄
   ▀▀        ▀▀▀▀▀    ▀▀        ▀▀       ▀▀▀▀▀▀▀▀     ▀▀▀▀",
    width: 60,
    min_area: (70, 16),
};
/// `toilet -f bigmono12 ferrit`: same canvas width as medium, but almost
/// twice the rows — denser and bolder rather than wider, so it needs
/// extra height more than extra width.
const WORDMARK_LARGE: Wordmark = Wordmark {
    art: "\
                                            ██
   ▒████                                    ██
   █████                                    ██       ██
   ██                                                ██
 ███████    ░████▒    ██░████   ██░████   ████     ███████
 ███████   ░██████▒   ███████   ███████   ████     ███████
   ██      ██▒  ▒██   ███░      ███░        ██       ██
   ██      ████████   ██        ██          ██       ██
   ██      ████████   ██        ██          ██       ██
   ██      ██         ██        ██          ██       ██
   ██      ███░  ▒█   ██        ██          ██       ██░
   ██      ░███████   ██        ██       ████████    █████
   ██       ░█████▒   ██        ██       ████████    ░████",
    width: 60,
    min_area: (70, 24),
};

/// Status pane's right side: lazygit's welcome screen, not a repo-status
/// view (see `docs/PLAN_1_LAYOUT.md`, "Welcome screen"). No repo data, so
/// this renders identically in `App::mock()` and against a real repo. Below
/// every tier's minimum area, the wordmark is dropped for a plain `ferrit`
/// label instead of wrapping into noise.
fn welcome_lines(width: u16, height: u16, accent: ratatui::style::Color) -> Vec<Line<'static>> {
    let fits = |w: &Wordmark| width >= w.min_area.0 && height >= w.min_area.1;
    let wordmark = [WORDMARK_LARGE, WORDMARK_MEDIUM, WORDMARK_SMALL]
        .into_iter()
        .find(fits);

    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(wordmark) = wordmark {
        lines.extend(wordmark.art.lines().map(|line| {
            let padded = format!("{line:<0$}", wordmark.width);
            Line::styled(padded, Style::new().fg(accent)).centered()
        }));
    } else {
        lines.push(
            Line::styled(
                "ferrit",
                Style::new().fg(accent).add_modifier(Modifier::BOLD),
            )
            .centered(),
        );
    }
    lines.push(Line::raw(""));
    lines.push(Line::raw(env!("CARGO_PKG_DESCRIPTION")).centered());
    lines.push(Line::raw(""));
    let idle = Style::new().fg(theme::IDLE);
    lines.push(
        Line::styled(
            format!(
                "v{} \u{b7} {} \u{b7} {}",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_LICENSE"),
                env!("CARGO_PKG_AUTHORS"),
            ),
            idle,
        )
        .centered(),
    );
    lines.push(Line::styled(env!("CARGO_PKG_REPOSITORY"), idle).centered());
    lines.push(Line::raw(""));
    lines.push(Line::styled("Press ? for keybindings", idle).centered());
    lines
}

fn draw_right_pane(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    // Remembered for mouse-wheel routing: a wheel event over this rect scrolls
    // the diff, one over the left column moves the selection.
    app.set_right_area(area);

    let focused = Style::new()
        .fg(app.theme_config.color())
        .add_modifier(Modifier::BOLD);
    let idle = Style::new().fg(theme::IDLE);
    // Branches normally previews nothing (" Log "); once drilled into a
    // branch's commit list, a selected row shows a real diff, so the title
    // matches what the Commits pane calls the same view: " Patch ".
    let right_title =
        if app.focus == Pane::Branches && matches!(app.diff_view(), DiffView::Commit(..)) {
            " Patch "
        } else {
            app.focus.right_title()
        };
    let border = if app.right_focused() { focused } else { idle };

    // An image selection takes over the right pane; otherwise it is mock text.
    match app.preview_mut() {
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
        let Some((text, total, _stat)) =
            app.rendered_diff(focus.as_ref(), diff_area.width as usize)
        else {
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
        app.set_right_viewport(diff_area.height as usize);
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

    // Branches focused, not drilled in: the selected branch's own commits,
    // shown passively (lazygit's live branch -> log preview, no Enter
    // needed) as multi-line `git log`-style blocks (`theme::branch_log_block`)
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
                .flat_map(theme::branch_log_block)
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
        app.set_right_viewport(viewport);
        return;
    }

    // Status: lazygit's welcome screen, not repo data — same in mock and on
    // a real repo, so this comes before the mock/real split below.
    if app.focus == Pane::Status {
        let panel = Paragraph::new(welcome_lines(
            area.width,
            area.height,
            app.theme_config.color(),
        ))
        .block(block)
        .wrap(Wrap { trim: false });
        frame.render_widget(panel, area);
        return;
    }

    // `App::mock()`: the sample text. A real repo with nothing selected (no
    // files, no commits) just leaves the pane blank.
    if !app.is_mock() {
        frame.render_widget(Paragraph::new("").block(block), area);
        return;
    }

    // Status already returned above (the welcome screen shows in mock too).
    // Branches has no mock body either: `App::mock()` has no repo, so there
    // is nothing to preview or drill into (G7); the mock path matches that
    // by leaving it blank rather than showing a fake sample.
    let body = match app.focus {
        Pane::Status | Pane::Branches => "",
        Pane::Files => mock::RIGHT_DIFF,
        Pane::Commits => mock::RIGHT_COMMIT,
        Pane::Stash => mock::RIGHT_STASH,
    };

    let text: Text<'_> = match app.focus {
        Pane::Files | Pane::Commits => theme::diff_lines(body, None),
        _ => body.into(),
    };

    let panel = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

/// Files pane with a real diff selected: lazygit's own two-column split,
/// Unstaged Changes beside Staged Changes, in place of the single right
/// pane every other selection uses (`draw_right_pane`). Deliberately
/// simplified against that path: each side renders directly through
/// `theme::render_diff` / `render_delta`, bypassing `App::rendered_diff`'s
/// cache (it is keyed for one diff at a time) and skipping the `]` / `[`
/// hunk-focus highlight — the two columns just scroll together on the one
/// `app.right_scroll()`.
fn draw_command_log(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let git_user_name = app.git_user_name().map(str::to_owned);
    let [heading, panel_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    frame.render_widget(
        Paragraph::new("Infos").style(Style::new().fg(theme::IDLE)),
        heading,
    );

    let block = Panel::new()
        .border_style(Style::new().fg(theme::IDLE))
        .block();
    let inner = block.inner(panel_area);
    frame.render_widget(block, panel_area);

    let [first, second] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);
    app.set_author_click_area(Rect::ZERO);

    // The two newest commands ferrit ran (writes only; `@` lists everything),
    // oldest on top. A repo-free `App::mock()` keeps its fixed sample.
    let lines: Vec<Line<'static>> = if app.is_mock() {
        mock::COMMAND_LOG
            .iter()
            .map(|command| theme::log_line(command))
            .collect()
    } else {
        command_log::recent(2, false)
            .iter()
            .map(theme::command_line)
            .collect()
    };
    let first_line = lines.first().cloned().unwrap_or_default();
    if let Some(name) = git_user_name {
        let name = format!("👤 {name}");
        let name_width = u16::try_from(UnicodeWidthStr::width(name.as_str())).unwrap_or(u16::MAX);
        let [command_area, name_area] = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(name_width.min(first.width)),
        ])
        .areas(first);
        frame.render_widget(Paragraph::new(first_line), command_area);
        frame.render_widget(
            Paragraph::new(name)
                .alignment(Alignment::Right)
                .style(Style::new().fg(theme::IDLE)),
            name_area,
        );
        app.set_author_click_area(name_area);
    } else {
        frame.render_widget(Paragraph::new(first_line), first);
    }
    if let Some(line) = lines.get(1) {
        frame.render_widget(Paragraph::new(line.clone()), second);
    }
}

/// The bottom key-hint bar, or — while a discard or branch-delete has a
/// confirm pending — a `message  y yes  n / Esc cancel` prompt in its place
/// (the phase 6 "small popup, or a one-line prompt in the keybar region"
/// fallback, since there is no popup primitive for a plain yes/no yet).
/// Context-sensitive per focus (`docs/PLAN_8_BRANCHES.md`): the Branches
/// pane's own keys share letters with the default bar's (`d` deletes a
/// branch there, not a file's worktree change), so it swaps in
/// `mock::BRANCHES_KEYBAR` instead of silently keeping the wrong hints on
/// screen; so does Stash (`mock::STASH_KEYBAR`, `docs/PLAN_10_STASH.md`).
fn draw_keybar(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = if app.operation.is_some() {
        mock::OPERATION_KEYBAR
    } else if app.focus == Pane::Branches && !app.branches_drilled() {
        mock::BRANCHES_KEYBAR
    } else if app.focus == Pane::Stash {
        mock::STASH_KEYBAR
    } else if app.focus == Pane::Commits && !app.commits_drilled() {
        mock::COMMITS_KEYBAR
    } else {
        mock::KEYBAR
    };
    match app.confirm_message() {
        Some(message) => KeyBar::confirm(message).render(frame, area),
        None => KeyBar::hints(text).render(frame, area),
    }
}
