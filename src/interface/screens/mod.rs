//! `draw(frame, &app)`: the whole screen, top-level layout down to widgets.
//!
//! Pure rendering. It reads `App` and `mock`, never mutates, never touches a
//! terminal, so `tests/render.rs` can call it against a `TestBackend`.
//! Colours come from `theme`.

use self::landed::Landed;
use crate::interface::render_state::RenderState;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui_image::{Resize, StatefulImage};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::app::full_screens::FullScreen;
use crate::app::pane::{PANES, Pane};
use crate::app::sheet::Sheet;
use crate::app::views::{DiffView, PopupView};
use crate::app::{mock, row_lines};
use crate::git::command_log;
use crate::git::image::preview::Preview;
use crate::interface::components::ui::key_bar::KeyBar;
use crate::interface::components::ui::pane_list::PaneList;
use crate::interface::components::ui::panel::Panel;
use crate::interface::components::ui::scroll_bar::ScrollBar;
use crate::keybindings::hints::{self, Bar};
use crate::theme::palette::Palette;

pub mod dashboard;
mod dashboard_sheet;
mod diff;
pub(crate) mod git_config;
mod landed;
mod popups;
mod settings;
pub(crate) mod welcome;

/// What the terminal gets: the screen, then the theme's paint pass over it, which
/// turns the unset and the ANSI colours of the frame into the theme's own
/// (`docs/PLAN_18_THEMES.md`). `draw` alone keeps the colours the widgets chose
/// (the palette's `Red`, `Blue`...), which is what most tests look at.
pub fn draw_painted(frame: &mut Frame<'_>, app: &mut App) {
    draw(frame, app);
    if let Some(scheme) = app.theme.config.scheme() {
        scheme.paint(frame.buffer_mut(), app.prefs.color_depth);
    }
}

/// Render the full screen for the current `App` state.
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    // The animations, the toast, the image protocol and the diff cache are
    // `&mut` for the length of the frame and nothing else is: take them out of
    // `App`, draw from `&App` with them, put them back, then land what the frame
    // learned. While they are out, `app.render` is empty, so everything the frame
    // reads of them goes through `render`, never through `app`.
    let mut render = std::mem::take(&mut app.render);
    let mut landed = Landed::default();
    draw_into(frame, app, &mut render, &mut landed);
    app.render = render;
    app.land(landed);
}

fn draw_into(frame: &mut Frame<'_>, app: &App, render: &mut RenderState, landed: &mut Landed) {
    let area = frame.area();
    let palette = app.palette();

    let show_help = app.help.is_visible(&render.help);
    let keybar = match app.full_screen() {
        FullScreen::GitConfig => draw_git_config(frame, app, render, landed, area),
        FullScreen::Welcome => draw_welcome(frame, app, render, landed, area),
        FullScreen::None => draw_panes(frame, app, render, landed, area),
    };

    if show_help {
        let lines = app.help_lines();
        // Above the key bar, which shows the help's own keys.
        let above_bar = Rect {
            height: area.height.saturating_sub(keybar.height),
            ..area
        };
        let accent = app.theme.config.color();
        let (scroll, query, searching) = app.help.view_parts();
        let overlay_state = &mut render.help;
        let rows = popups::draw_help(
            frame,
            above_bar,
            popups::HelpView {
                accent,
                lines: &lines,
                scroll,
                overlay_state,
                query,
                searching,
                palette: &palette,
            },
        );
        landed.help_rows = Some(rows);
    }
    let accent = app.theme.config.color();
    match app.popup_view_with(Some(&mut render.commit)) {
        Some(
            PopupView::Commit(mut view)
            | PopupView::NewBranch(mut view)
            | PopupView::Stash(mut view)
            | PopupView::Name(mut view)
            | PopupView::Upstream(mut view)
            | PopupView::Askpass(mut view),
        ) => {
            popups::draw_commit(frame, area, &mut view, accent, &palette);
        },
        Some(PopupView::CommitAllConfirm(Some(state))) => {
            popups::draw_commit_all_confirm(frame, area, state, accent, &palette);
        },
        Some(PopupView::CreateRemote(view)) => {
            popups::draw_create_remote(frame, area, &view, accent, &palette);
        },
        Some(PopupView::Note(message)) => popups::draw_note(frame, area, message, &palette),
        Some(PopupView::Menu(view)) => popups::draw_menu(frame, area, &view, accent, &palette),
        Some(PopupView::CommandLog(view)) => {
            popups::draw_command_log_view(frame, area, &view, accent, &palette);
        },
        Some(PopupView::CommitAllConfirm(None)) | None => {},
    }
    if let Some(toast) = &mut render.toast {
        toast.render(frame, area, &palette);
    }
}

/// The five panes, the command log and the key bar; returns the key bar's area.
fn draw_panes(
    frame: &mut Frame<'_>,
    app: &App,
    render: &mut RenderState,
    landed: &mut Landed,
    area: Rect,
) -> Rect {
    let palette = app.palette();
    let log_rows = command_log_rows(app, area.height);
    let [content, log, keybar] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(log_rows + 4),
        Constraint::Length(1),
    ])
    .areas(area);

    // LazyGit splits the diff only for partially staged files. One-sided
    // changes use the full-width diff pane.
    // A directory row never splits (nor narrows the side column): it shows one side.
    let files_split = app.nav.focus == Pane::Files
        && !app.files_selection_is_dir()
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

    draw_left_column(frame, app, landed, left);
    if files_split {
        diff::draw_files_columns(frame, app, landed, right);
    } else if app.nav.focus == Pane::Files && matches!(app.diff_view(), DiffView::Files(_)) {
        diff::draw_single_file_diff(frame, app, landed, right);
    } else {
        draw_right_pane(frame, app, render, landed, right);
    }
    draw_command_log(frame, app, landed, log);
    draw_keybar(frame, keybar, app, render, landed);

    if render.sheet.is_closed() {
        landed.settings_hits = Some(crate::app::settings::SettingsHits::default());
    } else {
        match app.sheets.kind {
            Sheet::Settings => settings::draw(frame, area, app, &palette, render, landed),
            Sheet::Dashboard => {
                // Above the key bar, which stays the dashboard's own.
                let above = Rect {
                    height: area.height.saturating_sub(keybar.height),
                    ..area
                };
                dashboard_sheet::draw(frame, above, app, render, landed);
            },
        }
    }
    keybar
}

/// The welcome screen above its key bar; returns the key bar's area.
fn draw_welcome(
    frame: &mut Frame<'_>,
    app: &App,
    render: &RenderState,
    landed: &mut Landed,
    area: Rect,
) -> Rect {
    let [page, keybar] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    if let Some(dir) = app.welcome_dir() {
        let view = welcome::View {
            dir,
            selected: app.welcome_selected(),
            palette: app.palette(),
            accent: app.theme.config.color(),
        };
        welcome::draw(frame, page, &view);
    }
    draw_keybar(frame, keybar, app, render, landed);
    keybar
}

/// The git config screen above its key bar; returns the key bar's area.
fn draw_git_config(
    frame: &mut Frame<'_>,
    app: &App,
    render: &RenderState,
    landed: &mut Landed,
    area: Rect,
) -> Rect {
    let [page, keybar] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let screen = app.git_config();
    let view = git_config::View {
        rows: &screen.rows,
        selected: screen.selected,
        offset: screen.offset(),
        scope: screen.scope,
        total: screen.total(),
        filter: &screen.filter,
        filtering: screen.filtering,
        note: screen.note.as_deref(),
        palette: app.palette(),
    };
    let offset = git_config::draw(frame, page, &view);
    landed.git_config_offset = Some(offset);
    draw_keybar(frame, keybar, app, render, landed);
    keybar
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
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

fn draw_left_column(frame: &mut Frame<'_>, app: &App, landed: &mut Landed, area: Rect) {
    let palette = &app.palette();
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
    let focus_index = DYNAMIC.iter().position(|&p| p == app.nav.focus);

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
        landed.left.push((pane, row));

        let focused = app.nav.focus == pane && !app.right_focused();
        let border = if focused {
            Style::new()
                .fg(app.theme.config.color())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(palette.idle)
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
                    .fg(app.theme.config.color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(palette.idle)
            },
        );

        let mut panel = Panel::new().title(title).border_style(border);
        if let Some((cur, total)) = app.counter(pane) {
            panel = panel.bottom_title(row_lines::counter_line(palette, cur, total));
        }
        let block = panel.block();

        let row_ct = app.row_count(pane);
        let mut lines = pane_lines(app, pane);
        let mut highlight = row_lines::selection_style(palette, focused);
        if pane == Pane::Files && focused {
            // Files rows carry a staging colour that the bar must not repaint.
            highlight.fg = None;
            if let Some(line) = lines.get_mut(app.selected(pane)) {
                row_lines::keep_colours_on_selection(palette, line);
            }
        }
        let detached = app.view_detached(pane);
        let offset = PaneList::new(lines, block)
            .detached(detached)
            .selected((row_ct > 0).then(|| app.selected(pane).min(row_ct - 1)))
            .offset(app.list_offset(pane))
            .highlight_style(highlight)
            .scrollbar_style(border)
            .render(frame, row);
        // Ratatui may have moved the offset to keep the selection on screen;
        // copy it back so a click in a scrolled list maps to the right row.
        landed.list_offset.push((pane, offset));
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

/// What ferrit is, and the promise it makes (the README opens with the same two
/// lines). The crate description is written for crates.io, not for this pane.
const TAGLINE_WHAT: &str = "The everyday git manager for your terminal";
const TAGLINE_PROMISE: &str = "Live in your repository. Start a GitHub project from nothing.";

/// Status pane's right side: lazygit's welcome screen, not a repo-status
/// view (see `docs/PLAN_1_LAYOUT.md`, "Welcome screen"). No repo data, so
/// this renders identically in `App::mock()` and against a real repo. Below
/// every tier's minimum area, the wordmark is dropped for a plain `ferrit`
/// label instead of wrapping into noise.
fn welcome_lines(
    width: u16,
    height: u16,
    accent: ratatui::style::Color,
    palette: &Palette,
) -> Vec<Line<'static>> {
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
    lines.push(
        Line::styled(
            TAGLINE_WHAT,
            Style::new().fg(accent).add_modifier(Modifier::BOLD),
        )
        .centered(),
    );
    lines.push(Line::raw(TAGLINE_PROMISE).centered());
    lines.push(Line::raw(""));
    let idle = Style::new().fg(palette.idle);
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

fn draw_right_pane(
    frame: &mut Frame<'_>,
    app: &App,
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
    // needed) as multi-line `git log`-style blocks (`row_lines::branch_log_block`)
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
                .flat_map(|commit| row_lines::branch_log_block(palette, commit))
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
        Pane::Files | Pane::Commits => row_lines::diff_lines(palette, body, None),
        _ => body.into(),
    };

    let panel = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}

/// Files pane with a real diff selected: lazygit's own two-column split,
/// Unstaged Changes beside Staged Changes, in place of the single right
/// pane every other selection uses (`draw_right_pane`). Deliberately
/// simplified against that path: each side renders directly through
/// `row_lines::render_diff` / `render_delta`, bypassing `App::rendered_diff`'s
/// cache (it is keyed for one diff at a time) and skipping the `]` / `[`
/// hunk-focus highlight — the two columns just scroll together on the one
/// `app.right_scroll()`.
fn draw_command_log(frame: &mut Frame<'_>, app: &App, landed: &mut Landed, area: Rect) {
    let palette = &app.palette();
    let git_user_name = app.git_user_name().map(str::to_owned);
    let [heading, panel_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    frame.render_widget(
        Paragraph::new("Infos").style(Style::new().fg(palette.idle)),
        heading,
    );

    let block = Panel::new()
        .border_style(Style::new().fg(palette.idle))
        .block();
    let inner = block.inner(panel_area);
    frame.render_widget(block, panel_area);

    let [first, rest, dashboard_row] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    landed.author = Some(Rect::ZERO);
    landed.dashboard = Some(Rect::ZERO);

    // The two newest commands ferrit ran (writes only; `@` lists everything),
    // oldest on top. A repo-free `App::mock()` keeps its fixed sample.
    let rows = usize::from(rest.height);
    let lines: Vec<Line<'static>> = if app.is_mock() {
        mock::COMMAND_LOG
            .iter()
            .map(|command| row_lines::log_line(palette, command))
            .collect()
    } else {
        let mut lines = command_log_lines(app);
        let extra = lines.len().saturating_sub(rows);
        lines.drain(..extra);
        lines
    };
    let first_line = lines.first().cloned().unwrap_or_default();
    if let Some(name) = git_user_name {
        let author = format!("👤 {name}");
        let author_width =
            u16::try_from(UnicodeWidthStr::width(author.as_str())).unwrap_or(u16::MAX);
        let [command_area, author_area] = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(author_width.min(first.width)),
        ])
        .areas(first);
        frame.render_widget(Paragraph::new(first_line), command_area);
        frame.render_widget(
            Paragraph::new(author)
                .alignment(Alignment::Right)
                .style(Style::new().fg(palette.idle)),
            author_area,
        );
        landed.author = Some(author_area);
    } else {
        frame.render_widget(Paragraph::new(first_line), first);
    }
    if lines.len() > 1 {
        frame.render_widget(
            Paragraph::new(lines.get(1..).unwrap_or_default().to_vec()),
            rest,
        );
    }
    let trigger = "📊 Dashboard";
    let trigger_width = u16::try_from(UnicodeWidthStr::width(trigger)).unwrap_or(u16::MAX);
    let [_, dashboard_area] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(trigger_width.min(dashboard_row.width)),
    ])
    .areas(dashboard_row);
    frame.render_widget(
        Paragraph::new(trigger)
            .alignment(Alignment::Right)
            .style(Style::new().fg(palette.idle)),
        dashboard_area,
    );
    landed.dashboard = Some(dashboard_area);
}

/// Inner rows of the Infos box: the two it always has, grown to hold the newest
/// command and every line git answered with (`MAX_OUTPUT_LINES` at most), but never
/// more than a third of the screen so the panes above keep their room.
fn command_log_rows(app: &App, screen_height: u16) -> u16 {
    if app.is_mock() {
        return 2;
    }
    let newest = command_log::recent(1, app.prefs.config.log.show_reads)
        .last()
        .map_or(0, |record| {
            row_lines::command_lines(&app.palette(), record).len()
        });
    let wanted = u16::try_from(newest).unwrap_or(u16::MAX).max(2);
    wanted.min((screen_height / 3).saturating_sub(3).max(2))
}

/// The two newest commands' lines, each command followed by git's answer.
fn command_log_lines(app: &App) -> Vec<Line<'static>> {
    command_log::recent(2, app.prefs.config.log.show_reads)
        .iter()
        .flat_map(|record| row_lines::command_lines(&app.palette(), record))
        .collect()
}

/// The bottom key-hint bar, or, while a discard or branch-delete has a
/// confirm pending, a `message  y yes  n / Esc cancel` prompt in its place.
/// The hints are generated from the live keymap (`app::hints`), so a remapped
/// key shows as remapped, and are cut to the terminal's width. Context
/// sensitive: an operation stopped mid-way wins, then the focused pane's own
/// keys (`d` means delete, discard or drop depending on the pane). The help
/// screen wins over all of it: only its own keys work while it is up.
fn draw_keybar(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    render: &RenderState,
    landed: &mut Landed,
) {
    let palette = app.palette();
    let bar = if app.help.is_visible(&render.help) {
        Bar::Help
    } else if app.dashboard_open_in(&render.sheet) {
        Bar::Dashboard
    } else if app.full_screen() == FullScreen::GitConfig {
        Bar::GitConfig
    } else if app.full_screen() == FullScreen::Welcome {
        Bar::Welcome
    } else if app.snapshot.operation.is_some() {
        Bar::Operation
    } else if app.right_focused() {
        Bar::RightPane
    } else if (app.nav.focus == Pane::Branches && app.branches_drilled())
        || (app.nav.focus == Pane::Commits && app.commits_drilled())
    {
        Bar::Drilled
    } else if app.nav.focus == Pane::Branches {
        Bar::Branches
    } else if app.nav.focus == Pane::Stash && app.row_count(Pane::Stash) == 0 {
        Bar::StashEmpty
    } else if app.nav.focus == Pane::Stash {
        Bar::Stash
    } else if app.nav.focus == Pane::Commits {
        Bar::Commits
    } else if app.row_count(Pane::Files) == 0 {
        Bar::FilesEmpty
    } else {
        Bar::Default
    };
    if let Some(message) = app.confirm_message() {
        KeyBar::confirm(message, &palette).render(frame, area);
        return;
    }
    let layout = hints::keybar_layout(&app.prefs.keymap, bar, usize::from(area.width));
    KeyBar::hints(&layout.text, &palette).render(frame, area);
    landed.keybar = Some((area, layout.hits));
}
