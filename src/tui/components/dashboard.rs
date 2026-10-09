//! The dashboard sheet: statistics, its state, its keys, and how it is drawn.

use crate::config::settings::SettingsSheet;
use crate::git::Snapshot;
use crate::git::port::GitPort;
use crate::git::stats::{NUMSTAT_CAP, RepoStats, StatsOptions, WALK_CAP, Window};
use crate::tui::components::dashboard::text::{date, thousands, window_label};
use crate::tui::draw::{Landed, RenderState, unix_now};
use crate::tui::error::AppError;
use crate::tui::event::Event;
use crate::tui::events::AppEvent;
use crate::tui::scene::Scene;
use crate::tui::widgets::charts::{ChartMode, ChartPalette, charts_mode_from_env};
use crate::tui::widgets::chrome::Drawer;
use crate::tui::widgets::chrome::Panel;
use crate::tui::widgets::chrome::ScrollBar;
use crate::tui::workers::{WorkerKind, run_worker};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::layout::Position;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;

/// A fingerprint of what the statistics depend on that the snapshot already
/// tells: the checked-out branch, the newest commit, every branch's tip time and
/// counts, the stash count. When it moves, the cache is dropped.
fn refs_fingerprint(snapshot: &Snapshot) -> u64 {
    let mut hasher = DefaultHasher::new();
    snapshot.header.branch.hash(&mut hasher);
    snapshot
        .commits
        .first()
        .map(|c| &c.full_hash)
        .hash(&mut hasher);
    for branch in &snapshot.branches {
        (&branch.name, branch.tip_time, branch.ahead, branch.behind).hash(&mut hasher);
    }
    snapshot.stashes.len().hash(&mut hasher);
    hasher.finish()
}

/// What the dashboard needs of the app to ask for statistics.
pub(crate) struct StatsCtx<'a> {
    /// What the last refresh looked like.
    pub(crate) snapshot: &'a Snapshot,
    /// The repository, if there is one; a worker opens its own handle on it.
    pub(crate) repo: Option<&'a dyn GitPort>,
    /// The way back onto the event channel, once the run loop has one.
    pub(crate) sender: Option<mpsc::Sender<AppEvent>>,
    /// The dashboard is up (sliding in or in): an error is shown only then.
    pub(crate) open: bool,
}

impl Dashboard {
    /// The sheet is about to open: at the top, no old error, statistics asked for.
    pub(crate) fn prepare(&mut self, ctx: &StatsCtx<'_>) {
        self.error = None;
        self.scroll = 0;
        self.ensure_stats(false, ctx);
    }

    /// The renderer's word on how far the page scrolls: keep the offset in it.
    pub(crate) fn clamp_scroll(&mut self, max: usize) {
        self.scroll = self.scroll.min(max);
    }

    /// Make sure the current window has statistics, or is on its way to have
    /// them. `force` recomputes even when the cache is fresh (`r`).
    fn ensure_stats(&mut self, force: bool, ctx: &StatsCtx<'_>) {
        let fingerprint = refs_fingerprint(ctx.snapshot);
        if fingerprint != self.fingerprint {
            self.cache = std::array::from_fn(|_| None);
            self.fingerprint = fingerprint;
        }
        let slot = self.window;
        if !force && self.cached().is_some_and(|c| !c.churn_pending) {
            return;
        }
        if !force && self.in_flight == Some(slot) {
            return;
        }
        if force {
            self.store(slot, None);
        }
        self.cancel_running();
        self.generation += 1;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.in_flight = Some(slot);
        self.error = None;

        let Some(repo) = ctx.repo.and_then(|repo| repo.reopen().ok()) else {
            self.in_flight = None;
            return;
        };
        let generation = self.generation;
        let window = self.window();
        let cancel = Arc::clone(&self.cancel);
        let Some(sender) = ctx.sender.clone() else {
            // No event loop (`App::mock`, a test without `run()`): do it now.
            for full in [false, true] {
                let result = read_stats(repo.as_ref(), window, full, &cancel);
                self.on_done(
                    StatsCompletion {
                        generation,
                        window,
                        full,
                        result,
                    },
                    ctx.open,
                );
            }
            return;
        };
        self.worker = Some(thread::spawn(move || {
            for full in [false, true] {
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                let result = read_stats(repo.as_ref(), window, full, &cancel);
                let failed = result.is_err();
                let _ = sender.send(AppEvent::StatsDone(StatsCompletion {
                    generation,
                    window,
                    full,
                    result,
                }));
                if failed {
                    break;
                }
            }
        }));
    }

    /// `AppEvent::StatsDone` arrived. A stale generation is dropped; a good
    /// result is cached whether or not the screen is still up, an error only
    /// shows when it is (`open`).
    pub(crate) fn on_done(&mut self, completion: StatsCompletion, open: bool) {
        if completion.generation != self.generation {
            return;
        }
        let Some(slot) = WINDOWS.iter().position(|w| *w == completion.window) else {
            return;
        };
        match completion.result {
            Ok(stats) => {
                self.store(
                    slot,
                    Some(Cached {
                        stats: *stats,
                        churn_pending: !completion.full,
                    }),
                );
                self.error = None;
            },
            Err(message) => {
                if open {
                    self.error = Some(message.to_string());
                }
            },
        }
        if completion.full || self.error.is_some() {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            self.in_flight = None;
        }
    }

    /// Every key while the dashboard is up (after the popups, a pending
    /// confirmation and the help overlay, which own input before it). `toggles`
    /// is whether the key is the one that opens the dashboard, which closes it
    /// whatever it is bound to.
    pub(crate) fn key(&mut self, key: KeyEvent, toggles: bool, ctx: &StatsCtx<'_>) -> Vec<Event> {
        if toggles {
            return vec![Event::CloseDashboard];
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return vec![Event::CloseDashboard],
            KeyCode::Char('?') => return vec![Event::OpenHelp],
            KeyCode::Char('t') => self.cycle_window(true, ctx),
            KeyCode::Char('T') => self.cycle_window(false, ctx),
            KeyCode::Char('r') => self.ensure_stats(true, ctx),
            KeyCode::Char('n') => self.show_counts = !self.show_counts,
            KeyCode::Char('k') | KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Char('j') | KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(PAGE),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(PAGE),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = usize::MAX,
            _ => {},
        }
        Vec::new()
    }

    /// The wheel scrolls three rows a notch; a left click outside the drawer
    /// closes it (the panes behind are dimmed and not clickable).
    pub(crate) fn mouse(&mut self, ev: MouseEvent, overlay: Option<Rect>) -> Vec<Event> {
        let point = Position::new(ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left)
                if !overlay.is_some_and(|rect| rect.contains(point)) =>
            {
                return vec![Event::CloseDashboard];
            },
            MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_sub(WHEEL_ROWS),
            MouseEventKind::ScrollDown => self.scroll = self.scroll.saturating_add(WHEEL_ROWS),
            _ => {},
        }
        Vec::new()
    }

    fn cycle_window(&mut self, forward: bool, ctx: &StatsCtx<'_>) {
        let count = WINDOWS.len();
        self.window = if forward {
            (self.window + 1) % count
        } else {
            (self.window + count - 1) % count
        };
        self.scroll = 0;
        self.ensure_stats(false, ctx);
    }
}

/// The windows `t` cycles through, shortest first.
pub(crate) const WINDOWS: [Window; 5] = [
    Window::Days7,
    Window::Days30,
    Window::Days90,
    Window::Year,
    Window::All,
];
/// `WINDOWS[2]`, 90 days: what the dashboard opens on.
pub(crate) const DEFAULT_WINDOW: usize = 2;
/// Rows `PageUp` / `PageDown` scroll.
pub(crate) const PAGE: usize = 10;
/// Rows one wheel notch scrolls.
pub(crate) const WHEEL_ROWS: usize = 3;

/// One answer of the worker.
#[derive(Debug)]
pub struct StatsCompletion {
    pub(crate) generation: u64,
    pub(crate) window: Window,
    /// The full pass (with the numstat), not the quick one.
    pub(crate) full: bool,
    pub(crate) result: Result<Box<RepoStats>, AppError>,
}

#[derive(Debug)]
pub(crate) struct Cached {
    pub(crate) stats: RepoStats,
    /// Only the quick pass is in: the lines and hot files are still coming.
    pub(crate) churn_pending: bool,
}

/// Everything the dashboard keeps between two frames.
#[derive(Debug)]
pub struct Dashboard {
    pub(crate) window: usize,
    pub(crate) cache: [Option<Cached>; WINDOWS.len()],
    /// Incremented by every request; a result of another generation is stale.
    pub(crate) generation: u64,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) worker: Option<JoinHandle<()>>,
    /// Which window the running request is for.
    pub(crate) in_flight: Option<usize>,
    /// What the cache was computed for: `refs_fingerprint` at request time.
    pub(crate) fingerprint: u64,
    pub(crate) scroll: usize,
    pub(crate) show_counts: bool,
    pub(crate) error: Option<String>,
}

impl Default for Dashboard {
    fn default() -> Self {
        Self {
            window: DEFAULT_WINDOW,
            cache: std::array::from_fn(|_| None),
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            worker: None,
            in_flight: None,
            fingerprint: 0,
            scroll: 0,
            show_counts: false,
            error: None,
        }
    }
}

impl Dashboard {
    pub fn window(&self) -> Window {
        WINDOWS.get(self.window).copied().unwrap_or(Window::Days90)
    }

    pub(crate) fn cached(&self) -> Option<&Cached> {
        self.cache.get(self.window).and_then(Option::as_ref)
    }

    /// Store or drop the statistics of one window.
    pub(crate) fn store(&mut self, slot: usize, value: Option<Cached>) {
        if let Some(entry) = self.cache.get_mut(slot) {
            *entry = value;
        }
    }

    /// The statistics for the current window, if any have arrived.
    pub fn stats(&self) -> Option<&RepoStats> {
        self.cached().map(|c| &c.stats)
    }

    /// The quick pass is in and the numstat is still being read.
    pub fn churn_pending(&self) -> bool {
        self.cached().is_some_and(|c| c.churn_pending)
    }

    /// Nothing at all has arrived for the current window yet.
    pub fn computing(&self) -> bool {
        self.cached().is_none() && self.in_flight.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Counts instead of percentages as the primary figure (`n`).
    pub fn show_counts(&self) -> bool {
        self.show_counts
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.in_flight.is_some()
    }

    /// Stop reading: set the flag the walk polls and let the thread finish on
    /// its own, nothing waits for it.
    pub(crate) fn cancel_running(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.worker = None;
        self.in_flight = None;
    }

    /// Quit: same, but wait, so the process does not exit under the thread.
    pub(crate) fn stop_and_join(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.in_flight = None;
    }
}

/// One pass of the statistics behind the panic boundary.
pub(crate) fn read_stats(
    repo: &dyn GitPort,
    window: Window,
    full: bool,
    cancel: &AtomicBool,
) -> Result<Box<RepoStats>, AppError> {
    let options = StatsOptions {
        churn: full,
        ..StatsOptions::default()
    };
    run_worker(WorkerKind::Stats, || {
        repo.stats_with(window, &options, cancel)
    })
    .map_err(AppError::from)
    .and_then(|result| result.map_err(AppError::from))
    .map(Box::new)
}

/// The drawer and what it can hold. One drawer state means one animation and,
/// by construction, one sheet at a time.
#[derive(Default)]
pub(crate) struct Sheets {
    /// Which sheet the drawer holds while it is not closed.
    pub(crate) kind: Sheet,
    pub(crate) settings: SettingsSheet,
    pub(crate) dashboard: Dashboard,
}

/// What the drawer holds. Kept while it slides out, so the last frames of the
/// close are still the sheet that was up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Sheet {
    /// Ferrit's own settings, opened by a click on the author's name.
    #[default]
    Settings,
    /// The repository dashboard, opened by `D` (`docs/PLAN_13_DASHBOARD.md`).
    Dashboard,
}

mod charts;
mod tables;
mod text;

/// Two columns from this width.
pub const WIDE: u16 = 110;
/// One column from this width; under it only the totals and the work in progress.
pub const NARROW: u16 = 60;
/// The page is never wider than this: past it the margins grow, not the charts.
pub const MAX_WIDTH: u16 = 110;
/// Columns between the two columns of the wide layout.
const GUTTER: u16 = 4;
/// Columns between the border and the content.
const PAD: u16 = 2;

/// Whether the page draws its own rounded border and its "Dashboard" title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chrome {
    /// A page of its own: the border and the title.
    Framed,
    /// Inside a frame that already has them (the dashboard sheet).
    Bare,
}

impl Chrome {
    /// The rows a border takes at the top, and again at the bottom.
    const fn rim(self) -> u16 {
        match self {
            Self::Framed => 1,
            Self::Bare => 0,
        }
    }
}

/// What the screen draws, all of it given: nothing here reads the clock or the app.
#[derive(Debug)]
pub struct View<'a> {
    /// `None` until the first pass arrives, or when it failed.
    pub stats: Option<&'a RepoStats>,
    pub repo: &'a str,
    /// The checked-out branch.
    pub branch: &'a str,
    pub colors: ChartPalette,
    pub mode: ChartMode,
    /// `n`: counts first, percentages in brackets.
    pub show_counts: bool,
    /// Nothing has arrived yet.
    pub computing: bool,
    /// The lines and hot files are still being read.
    pub churn_pending: bool,
    pub error: Option<&'a str>,
    pub scroll: usize,
    /// Draw the page's own rounded border and its "Dashboard" title. A sheet has
    /// its own frame and title, so it asks for none (`screens/dashboard_sheet.rs`).
    pub chrome: Chrome,
    /// Unix seconds: the end of the series and the base of relative times.
    pub now: i64,
}

/// A view with stats in hand.
pub(crate) struct Ctx<'a> {
    pub stats: &'a RepoStats,
    pub view: &'a View<'a>,
}

impl Ctx<'_> {
    pub(crate) fn colors(&self) -> &ChartPalette {
        &self.view.colors
    }

    pub(crate) fn dim(&self) -> Style {
        self.view.colors.dim
    }
}

/// One line of text at the top of `area`.
pub(crate) fn note(buf: &mut Buffer, area: Rect, text: &str, style: Style) {
    Paragraph::new(Line::styled(text.to_owned(), style)).render(area, buf);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Activity,
    Kinds,
    Heat,
    Contributors,
    Hot,
    Branches,
}

impl Section {
    /// The bold title and the dim words after it.
    fn title(self, ctx: &Ctx<'_>, width: u16) -> (&'static str, String) {
        match self {
            Self::Activity => ("Activity", charts::activity_unit(ctx)),
            Self::Kinds => ("What was done", String::new()),
            Self::Heat => ("Commits per day", charts::heat_unit(width)),
            Self::Contributors => ("Contributors", String::new()),
            Self::Hot => ("Hot files", tables::hot_unit(ctx).to_owned()),
            Self::Branches => ("Branches", String::new()),
        }
    }

    /// Content rows in a column `width` wide: the least that still reads, and
    /// what fills the section.
    fn rows(self, ctx: &Ctx<'_>, width: u16) -> (u16, u16) {
        let stats = ctx.stats;
        let rows = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
        match self {
            Self::Activity => (5, 8),
            Self::Kinds => (6, 8),
            Self::Heat => (5, 10),
            Self::Contributors => {
                let want = rows(stats.authors.len().min(6) + 1);
                (want.min(4), want)
            },
            Self::Hot => {
                let files = stats.hot_files.as_ref().map_or(1, |h| {
                    h.files.len().max(1) + tables::hot_footer(h, usize::from(width)).len()
                });
                let want = rows(files);
                (want.min(4), want)
            },
            Self::Branches => {
                let n = stats.branches.len();
                let want =
                    rows(1 + n.min(tables::BRANCH_ROWS) + usize::from(n > tables::BRANCH_ROWS));
                (want.min(5), want)
            },
        }
    }

    fn draw(self, ctx: &Ctx<'_>, area: Rect, buf: &mut Buffer) {
        match self {
            Self::Activity => charts::activity(ctx, area, buf),
            Self::Kinds => charts::kinds(ctx, area, buf),
            Self::Heat => charts::heat(ctx, area, buf),
            Self::Contributors => tables::contributors(ctx, area, buf),
            Self::Hot => tables::hot_files(ctx, area, buf),
            Self::Branches => tables::branches(ctx, area, buf),
        }
    }
}

/// One horizontal band of the page: one or two sections side by side.
struct Band {
    sections: Vec<Section>,
    rows: u16,
}

/// The bands for `wide` or stacked, each at the rows it wants; when `budget` is
/// given (wide) and short, the rows shrink towards the minimum.
fn bands(ctx: &Ctx<'_>, wide: bool, width: u16, budget: u16) -> Vec<Band> {
    let groups: Vec<Vec<Section>> = if wide {
        vec![
            vec![Section::Activity, Section::Kinds],
            vec![Section::Heat, Section::Contributors],
            vec![Section::Hot, Section::Branches],
        ]
    } else {
        [
            Section::Activity,
            Section::Kinds,
            Section::Heat,
            Section::Contributors,
            Section::Hot,
            Section::Branches,
        ]
        .into_iter()
        .map(|s| vec![s])
        .collect()
    };
    let sizes: Vec<(u16, u16)> = groups
        .iter()
        .map(|g| {
            g.iter().fold((0, 0), |(lo, hi), s| {
                let (a, b) = s.rows(ctx, width);
                (lo.max(a), hi.max(b))
            })
        })
        .collect();
    let mut rows: Vec<u16> = sizes.iter().map(|&(_, want)| want).collect();
    if wide {
        let mins: u16 = sizes.iter().map(|&(min, _)| min).sum();
        let wants: u16 = rows.iter().sum();
        if budget < wants {
            // Take rows back from the last band first, never under its minimum.
            let mut over = wants - budget.max(mins);
            for (row, &(min, _)) in rows.iter_mut().zip(&sizes).rev() {
                let take = over.min(row.saturating_sub(min));
                *row -= take;
                over -= take;
            }
        } else if let Some(first) = rows.first_mut() {
            // Spare room grows the line chart, within reason.
            *first += (budget - wants).min(8);
        }
    }
    groups
        .into_iter()
        .zip(rows)
        .map(|(sections, rows)| Band { sections, rows })
        .collect()
}

/// Lines under the totals: shallow clone, sampling, an error next to stats.
fn notices(ctx: &Ctx<'_>) -> Vec<Line<'static>> {
    let warn = Style::new().fg(ctx.colors().warn);
    let stats = ctx.stats;
    let mut out = Vec::new();
    if stats.shallow {
        let since = stats
            .totals
            .first_commit
            .map_or_else(|| "the first commit".to_owned(), date);
        out.push(Line::styled(
            format!("shallow: history before {since} is not available"),
            warn,
        ));
    }
    if stats.sampled {
        out.push(Line::styled(
            format!(
                "sampled: newest {} commits read, lines from the newest {}",
                thousands(WALK_CAP),
                thousands(NUMSTAT_CAP)
            ),
            warn,
        ));
    }
    if let Some(error) = ctx.view.error {
        out.push(Line::styled(format!("stats error: {error}"), warn));
    }
    out
}

/// The border of the whole page: one rounded line in the recessive rule colour.
fn frame_block(view: &View<'_>) -> Block<'static> {
    Panel::new().border_style(view.colors.rule).block()
}

/// `ferrit · main   Dashboard` on the left, `window: 90 days (t)` on the right.
fn header(view: &View<'_>, area: Rect, buf: &mut Buffer) {
    let dim = view.colors.dim;
    Paragraph::new(Line::from(vec![
        Span::styled(
            view.repo.to_owned(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", dim),
        Span::raw(view.branch.to_owned()),
        Span::styled(
            if view.chrome == Chrome::Framed {
                "   Dashboard"
            } else {
                ""
            },
            dim,
        ),
    ]))
    .render(area, buf);
    if let Some(stats) = view.stats {
        Line::styled(format!("window: {} (t)", window_label(stats.window)), dim)
            .right_aligned()
            .render(area, buf);
    }
}

/// Under `NARROW` columns: the totals and the work in progress, and a hint.
fn compact(view: &View<'_>, width: u16, height: u16) -> Buffer {
    let lines: Vec<Line<'static>> = match view.stats.map(|stats| Ctx { stats, view }) {
        Some(ctx) => vec![
            tables::totals_line(&ctx),
            tables::progress_line(&ctx),
            Line::styled("widen the terminal for charts", ctx.dim()),
        ],
        None => vec![Line::raw(match view.error {
            Some(error) => format!("could not read the statistics: {error}"),
            None => "computing…".to_owned(),
        })],
    };
    // The rows the border takes, top and bottom; the header is one more.
    let rim = view.chrome.rim();
    let chrome = 1 + 2 * rim;
    let page = chrome + u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let area = Rect::new(0, 0, width, page.max(u16::from(height > 0)));
    let mut buf = Buffer::empty(area);
    if view.chrome == Chrome::Framed {
        frame_block(view).render(area, &mut buf);
    }
    let inner_w = width.saturating_sub(2 * PAD);
    header(view, Rect::new(PAD, rim, inner_w, 1), &mut buf);
    Paragraph::new(lines).render(
        Rect::new(PAD, rim + 1, inner_w, page.saturating_sub(chrome)),
        &mut buf,
    );
    buf
}

/// The lines between the header and the first section: a blank, the stat
/// tiles, the notices, and the "no commits yet" of an empty repository.
fn body(view: &View<'_>, ctx: Option<&Ctx<'_>>, inner_w: u16) -> Vec<Line<'static>> {
    let blank = Line::raw("");
    let Some(ctx) = ctx else {
        return vec![
            blank,
            Line::raw(match view.error {
                Some(error) => format!("could not read the statistics: {error}"),
                None => "computing…".to_owned(),
            }),
        ];
    };
    let mut out = vec![blank.clone()];
    out.extend(tables::tiles(ctx, inner_w));
    out.push(blank.clone());
    let notices = notices(ctx);
    if !notices.is_empty() {
        out.extend(notices);
        out.push(blank.clone());
    }
    if ctx.stats.totals.first_commit.is_none() && ctx.stats.totals.commits == 0 {
        out.push(Line::styled("no commits yet", ctx.dim()));
        out.push(blank);
    }
    out
}

/// The whole page on its own buffer, `width` columns wide (already capped) and
/// as tall as its content (or `height`, when the content is shorter).
fn compose(view: &View<'_>, width: u16, height: u16) -> Buffer {
    if width < NARROW {
        return compact(view, width, height);
    }
    let ctx = view.stats.map(|stats| Ctx { stats, view });
    // The rows the border takes, top and bottom (none when the page is not framed).
    let rim = view.chrome.rim();
    let inner_w = width.saturating_sub(2 * PAD);
    let wide = width >= WIDE;
    let col_w = if wide {
        inner_w.saturating_sub(GUTTER) / 2
    } else {
        inner_w
    };
    let right_x = PAD + col_w + GUTTER;
    let right_w = inner_w.saturating_sub(col_w + GUTTER);
    let body = body(view, ctx.as_ref(), inner_w);
    let body_rows = u16::try_from(body.len()).unwrap_or(u16::MAX);
    let charts = ctx
        .as_ref()
        .is_some_and(|c| c.stats.totals.first_commit.is_some() || c.stats.totals.commits > 0);
    // The work in progress is one row under the last section.
    let progress_rows = u16::from(ctx.is_some());
    let bands = match &ctx {
        Some(ctx) if charts => {
            // Borders, header, body, a title, a rule and a blank per band, and
            // the progress line come off the height; the bands share the rest.
            let n = if wide { 3 } else { 6 };
            let chrome = 1 + 2 * rim + body_rows + 3 * n + progress_rows;
            bands(ctx, wide, col_w, height.saturating_sub(chrome))
        },
        _ => Vec::new(),
    };
    let band_rows: u16 = bands.iter().map(|b| b.rows + 3).sum();
    let page = 1 + 2 * rim + body_rows + band_rows + progress_rows;
    let area = Rect::new(0, 0, width, page.max(u16::from(height > 0)));
    let mut buf = Buffer::empty(area);
    if view.chrome == Chrome::Framed {
        frame_block(view).render(area, &mut buf);
    }
    header(view, Rect::new(PAD, rim, inner_w, 1), &mut buf);
    let mut y = rim + 1;
    for line in body {
        Paragraph::new(line).render(Rect::new(PAD, y, inner_w, 1), &mut buf);
        y += 1;
    }
    let Some(ctx) = &ctx else {
        return buf;
    };
    let rule = view.colors.rule;
    let bold = Style::new().add_modifier(Modifier::BOLD);
    for band in &bands {
        let columns = if band.sections.len() == 2 {
            vec![(PAD, col_w), (right_x, right_w)]
        } else {
            vec![(PAD, inner_w)]
        };
        for (section, (x, w)) in band.sections.iter().zip(columns) {
            let (title, unit) = section.title(ctx, w);
            let mut spans = vec![Span::styled(title, bold)];
            if !unit.is_empty() {
                spans.push(Span::styled(format!("  {unit}"), view.colors.dim));
            }
            Paragraph::new(Line::from(spans)).render(Rect::new(x, y, w, 1), &mut buf);
            Paragraph::new(Line::styled("─".repeat(usize::from(w)), rule))
                .render(Rect::new(x, y + 1, w, 1), &mut buf);
            section.draw(ctx, Rect::new(x, y + 2, w, band.rows), &mut buf);
        }
        y += band.rows + 3;
    }
    Paragraph::new(tables::progress_line(ctx)).render(Rect::new(PAD, y, inner_w, 1), &mut buf);
    buf
}

/// Draw the dashboard into `area` of `frame`, scrolled by `view.scroll`
/// (clamped). Returns the largest offset that shows anything new, for the app
/// to clamp its own scroll to.
pub fn draw(frame: &mut Frame<'_>, area: Rect, view: &View<'_>) -> usize {
    frame.render_widget(Clear, area);
    let page_w = area.width.min(MAX_WIDTH);
    let margin = (area.width - page_w) / 2;
    let page = compose(view, page_w, area.height);
    let page_rows = page.area.height;
    let max_scroll = usize::from(page_rows.saturating_sub(area.height));
    let scroll = view.scroll.min(max_scroll);
    let first = u16::try_from(scroll).unwrap_or(0);
    let out = frame.buffer_mut();
    for dy in 0..area.height.min(page_rows) {
        for x in 0..page_w {
            if let (Some(from), Some(to)) = (
                page.cell((x, dy + first)),
                out.cell_mut((area.x + margin + x, area.y + dy)),
            ) {
                to.clone_from(from);
            }
        }
    }
    let track = Rect::new(area.right().saturating_sub(1), area.y, 1, area.height);
    ScrollBar::new(usize::from(page_rows), usize::from(area.height), scroll)
        .style(Style::new().fg(view.colors.accent))
        .render(frame, track);
    max_scroll
}

/// The most of the terminal's width the drawer takes, in percent.
const MAX_PERCENT: u16 = 95;

/// Draw the drawer and the page in it, over `area`. The page's scroll is clamped
/// to what it can scroll.
pub(crate) fn draw_sheet(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &Scene<'_>,
    render: &mut RenderState,
    landed: &mut Landed,
) {
    let accent = app.theme.config.color();
    // The page plus the drawer's two border columns, no more.
    let width = (MAX_WIDTH + 2).min(area.width.saturating_mul(MAX_PERCENT) / 100);
    let Some(inner) = Drawer::new(&mut render.sheet, " Dashboard ")
        .width(Constraint::Length(width))
        .border_style(Style::new().fg(accent))
        .render(frame, area)
    else {
        return;
    };
    let view = View {
        stats: app.dashboard().stats(),
        repo: app.repo_name,
        branch: &app.snapshot.header.branch,
        colors: ChartPalette {
            density: std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()),
            ..ChartPalette::for_palette(&app.palette())
        },
        mode: charts_mode_from_env(),
        show_counts: app.dashboard().show_counts(),
        computing: app.dashboard().computing(),
        churn_pending: app.dashboard().churn_pending(),
        error: app.dashboard().error(),
        scroll: app.dashboard().scroll(),
        chrome: Chrome::Bare,
        now: unix_now(),
    };
    let max_scroll = draw(frame, inner, &view);
    landed.dashboard_max_scroll = Some(max_scroll);
}
