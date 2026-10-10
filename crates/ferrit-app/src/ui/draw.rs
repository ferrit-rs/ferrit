//! Drawing a frame: the top-level layout, what the frame learned (`Landed`), and what ratatui needs mutable.

use crate::ui::App;
use crate::ui::components::command_log::command_log_rows;
use crate::ui::components::command_log::draw_command_log;
use crate::ui::components::command_log::draw_command_log_view;
use crate::ui::components::commit_editor::view::draw_commit;
use crate::ui::components::commit_editor::view::draw_commit_all_confirm;
use crate::ui::components::create_remote::view::draw_create_remote;
use crate::ui::components::dashboard::Sheet;
use crate::ui::components::diff::draw::draw_files_columns;
use crate::ui::components::diff::draw::draw_right_pane;
use crate::ui::components::diff::draw::draw_single_file_diff;
use crate::ui::components::diff::queries::RightKey;
use crate::ui::components::diff::views::{DiffView, PopupView};
use crate::ui::components::help::HelpView;
use crate::ui::components::help::draw_help;
use crate::ui::components::keybar::KeybarHit;
use crate::ui::components::keybar::view::draw_keybar;
use crate::ui::components::menu::draw_menu;
use crate::ui::components::panes::draw::draw_left_column;
use crate::ui::components::panes::nav::Pane;
use crate::ui::components::popups::draw_note;
use crate::ui::components::settings::SettingsHits;
use crate::ui::components::welcome::draw_welcome;
use crate::ui::components::{dashboard, git_config, settings};
use crate::ui::image::preview::Preview;
use crate::ui::scene::Scene;
use ferrit_tui::widgets::toast::Toast;
use ferrit_tui::widgets::tui_overlay::state::OverlayState;
use ratatui::Frame;
use ratatui::crossterm::event::MouseEvent;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Text;
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;

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
    {
        let scene = app.scene();
        draw_into(frame, &scene, &mut render, &mut landed);
    }
    app.render = render;
    app.land(landed);
}

fn draw_into(
    frame: &mut Frame<'_>,
    app: &Scene<'_>,
    render: &mut RenderState,
    landed: &mut Landed,
) {
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
        let rows = draw_help(
            frame,
            above_bar,
            HelpView {
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
            draw_commit(frame, area, &mut view, accent, &palette);
        },
        Some(PopupView::CommitAllConfirm(Some(state))) => {
            draw_commit_all_confirm(frame, area, state, accent, &palette);
        },
        Some(PopupView::CreateRemote(view)) => {
            draw_create_remote(frame, area, &view, accent, &palette);
        },
        Some(PopupView::Note(message)) => draw_note(frame, area, message, &palette),
        Some(PopupView::Menu(view)) => draw_menu(frame, area, &view, accent, &palette),
        Some(PopupView::CommandLog(view)) => {
            draw_command_log_view(frame, area, &view, accent, &palette);
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
    app: &Scene<'_>,
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
        draw_files_columns(frame, app, landed, right);
    } else if app.nav.focus == Pane::Files && matches!(app.diff_view(), DiffView::Files(_)) {
        draw_single_file_diff(frame, app, landed, right);
    } else {
        draw_right_pane(frame, app, render, landed, right);
    }
    draw_command_log(frame, app, landed, log);
    draw_keybar(frame, keybar, app, render, landed);

    if render.sheet.is_closed() {
        landed.settings_hits = Some(SettingsHits::default());
    } else {
        match app.sheets.kind {
            Sheet::Settings => settings::draw(frame, area, app, &palette, render, landed),
            Sheet::Dashboard => {
                // Above the key bar, which stays the dashboard's own.
                let above = Rect {
                    height: area.height.saturating_sub(keybar.height),
                    ..area
                };
                dashboard::view::draw_sheet(frame, above, app, render, landed);
            },
        }
    }
    keybar
}

/// The git config screen above its key bar; returns the key bar's area.
fn draw_git_config(
    frame: &mut Frame<'_>,
    app: &Scene<'_>,
    render: &RenderState,
    landed: &mut Landed,
    area: Rect,
) -> Rect {
    let [page, keybar] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let screen = app.git_config();
    let view = git_config::draw::View {
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
    let offset = git_config::draw::draw(frame, page, &view);
    landed.git_config_offset = Some(offset);
    draw_keybar(frame, keybar, app, render, landed);
    keybar
}

pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// What ferrit is, and the promise it makes (the README opens with the same two
/// lines). The crate description is written for crates.io, not for this pane.
pub(crate) const TAGLINE_WHAT: &str = "The everyday git manager for your terminal";
pub(crate) const TAGLINE_PROMISE: &str =
    "Live in your repository. Start a GitHub project from nothing.";

/// `None` and empty mean "this frame did not touch it": the previous value stays.
#[derive(Default)]
pub(crate) struct Landed {
    /// A left pane's bordered rect.
    pub(crate) left: Vec<(Pane, Rect)>,
    /// A left pane's list offset after ratatui scrolled it to keep the selection in view.
    pub(crate) list_offset: Vec<(Pane, usize)>,
    /// The whole right column.
    pub(crate) right_area: Option<Rect>,
    /// Inner height of the right pane's diff box.
    pub(crate) right_viewport: Option<usize>,
    /// The author's name in the info panel (`Rect::ZERO` when not drawn).
    pub(crate) author: Option<Rect>,
    /// The Dashboard trigger beside it.
    pub(crate) dashboard: Option<Rect>,
    /// The key bar's rect and what each part of it runs.
    pub(crate) keybar: Option<(Rect, Vec<KeybarHit>)>,
    /// The settings sheet's clickable parts.
    pub(crate) settings_hits: Option<SettingsHits>,
    /// The settings sheet's scroll after keeping the selected row in view, and
    /// whether that following has now been done.
    pub(crate) settings_scroll: Option<(usize, bool)>,
    /// The git config screen's first visible row.
    pub(crate) git_config_offset: Option<usize>,
    /// How many help lines fit.
    pub(crate) help_rows: Option<usize>,
    /// How far the dashboard page scrolls, at most.
    pub(crate) dashboard_max_scroll: Option<usize>,
}

pub(crate) struct RenderState {
    /// The help dialog.
    pub(crate) help: OverlayState,
    /// The side drawer, whichever sheet it holds.
    pub(crate) sheet: OverlayState,
    /// The backdrop of the commit editor and of the "stage everything?" question.
    pub(crate) commit: OverlayState,
    /// The bottom-right error notification, dismissed by its `x`, `Esc` or a timeout.
    pub(crate) toast: Option<Toast>,
    /// The right pane's image: a live protocol that resizes and re-encodes itself
    /// at render time, through `&mut`.
    pub(crate) preview: Preview,
    /// The styled commit diff, kept so scrolling does not rerun syntax
    /// highlighting. Keyed by the selection, the diff text, the focus range and
    /// the pane width.
    pub(crate) diff_cache: Option<RenderedDiff>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            help: OverlayState::new().with_duration(Duration::from_millis(180)),
            sheet: OverlayState::new().with_duration(Duration::from_millis(200)),
            commit: OverlayState::new(),
            toast: None,
            preview: Preview::None,
            diff_cache: None,
        }
    }
}

pub(crate) struct RenderedDiff {
    pub(crate) key: Option<RightKey>,
    pub(crate) source: String,
    pub(crate) focus: Option<Range<usize>>,
    pub(crate) width: usize,
    pub(crate) text: Text<'static>,
}

impl RenderState {
    /// Count the error toast's timeout and animation, and drop it once closed.
    pub(crate) fn tick_toast(&mut self, elapsed: Duration) {
        if let Some(toast) = &mut self.toast {
            toast.tick(elapsed);
            if toast.is_closed() {
                self.toast = None;
            }
        }
    }

    /// Start closing the toast. `true` when there was one to close.
    pub(crate) fn dismiss_toast(&mut self) -> bool {
        match &mut self.toast {
            Some(toast) if !toast.is_closing() => {
                toast.dismiss();
                true
            },
            _ => false,
        }
    }
}

/// Which of the animated things were moving at one instant.
#[derive(Clone, Copy)]
pub(crate) struct Animating {
    sheet: bool,
    help: bool,
    toast: bool,
}

impl Animating {
    pub(crate) const fn any(self) -> bool {
        self.sheet || self.help || self.toast
    }
}

impl RenderState {
    pub(crate) fn animating(&self) -> Animating {
        Animating {
            sheet: self.sheet.is_animating(),
            help: self.help.is_animating(),
            toast: self.toast.as_ref().is_some_and(Toast::is_animating),
        }
    }

    /// Advance every animation and the toast's timeout by `elapsed`.
    pub(crate) fn tick(&mut self, elapsed: Duration) {
        self.sheet.tick(elapsed);
        self.help.tick(elapsed);
        self.tick_toast(elapsed);
    }

    /// The same at the end of a batch of events: a sheet or the help that began
    /// to animate during the batch waits for its first frame, so only the ones
    /// already moving (`was`) advance.
    pub(crate) fn tick_after_batch(&mut self, elapsed: Duration, was: Animating) {
        if self.sheet.is_animating() && was.sheet {
            self.sheet.tick(elapsed);
        }
        if self.help.is_animating() && was.help {
            self.help.tick(elapsed);
        }
        self.tick_toast(elapsed);
    }

    /// Give a mouse event to the toast. `true` when it consumed it.
    pub(crate) fn toast_mouse(&mut self, event: MouseEvent) -> bool {
        self.toast
            .as_mut()
            .is_some_and(|toast| toast.on_mouse(event))
    }
}

#[derive(Default)]
pub(crate) struct FullScreens {
    /// The view showing, if any.
    pub(crate) active: FullScreen,
    pub(crate) git_config: git_config::screen::GitConfigScreen,
    /// The folder the welcome screen is about; `None` once there is a repository.
    pub(crate) welcome_dir: Option<PathBuf>,
    /// The highlighted row of the welcome screen: 0 is `git init`, 1 is quit.
    pub(crate) welcome_selected: usize,
}

/// A view that takes the whole terminal in place of the five panes: the git
/// config editor (`docs/PLAN_14_GIT_CONFIG.md`) and the welcome screen. (The
/// dashboard was one until phase 19: it is a sheet now, `app::sheet`.)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FullScreen {
    #[default]
    None,
    GitConfig,
    /// No repository: ferrit started in a folder that is not one
    /// (`docs/PLAN_16_START_WITHOUT_REPO.md`).
    Welcome,
}
