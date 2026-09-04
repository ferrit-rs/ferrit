//! Application state and the draw / event loop.
//!
//! Phase 2 wired the Status and Files panes to a real read-only `git::Repo`.
//! `App` owns the repo handle, the cached snapshot, which left pane is focused,
//! and one selection cursor per pane. Branches, Commits and Stash still read
//! from `mock` until G3..G5. `App::mock()` is the repo-free path the render
//! tests use.

use std::path::Path;

use color_eyre::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::text::Line;
use ratatui_image::picker::Picker;

use crate::git::{self, GitResult};
use crate::preview::{self, Preview};
use crate::tui::Tui;
use crate::{mock, theme, ui};

/// The five left panes, in top-to-bottom screen order.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Pane {
    #[default]
    Status,
    Files,
    Branches,
    Commits,
    Stash,
}

/// Panes in order. Index into this is also the index into `App::selection`.
pub const PANES: [Pane; 5] = [
    Pane::Status,
    Pane::Files,
    Pane::Branches,
    Pane::Commits,
    Pane::Stash,
];

impl Pane {
    /// Position in `PANES`, used to index `App::selection`.
    pub fn index(self) -> usize {
        PANES.iter().position(|&p| p == self).unwrap()
    }

    /// Bordered-box title, including the digit that focuses it.
    pub fn title(self) -> &'static str {
        match self {
            Pane::Status => "1 Status",
            Pane::Files => "2 Files",
            Pane::Branches => "3 Local Branches",
            Pane::Commits => "4 Commits",
            Pane::Stash => "5 Stash",
        }
    }
}

pub struct App {
    /// Which left pane has focus.
    pub focus: Pane,
    /// Selection cursor per pane, indexed by `Pane::index`.
    pub selection: [usize; 5],
    /// Whether the help overlay is up.
    pub show_help: bool,
    should_quit: bool,

    /// `None` in `App::mock()`; otherwise the open repository.
    repo: Option<git::Repo>,
    header: git::StatusHeader,
    files: Vec<git::FileEntry>,
    /// Last `refresh()` failure, shown in the Status pane. Never a panic.
    last_error: Option<String>,

    /// Terminal graphics backend for the image preview. Starts on half-blocks
    /// (works everywhere); `detect_graphics()` upgrades it to sixel / kitty /
    /// iterm2 when the real terminal supports one.
    picker: Picker,
    /// Right-pane image preview for the current selection, rebuilt on nav.
    preview: Preview,
}

impl App {
    fn base(repo: Option<git::Repo>) -> Self {
        Self {
            focus: Pane::default(),
            selection: [0; 5],
            show_help: false,
            should_quit: false,
            repo,
            header: git::StatusHeader::default(),
            files: Vec::new(),
            last_error: None,
            picker: Picker::halfblocks(),
            preview: Preview::None,
        }
    }

    /// Open the repo at or above `path`, then take one snapshot.
    pub fn open(path: &Path) -> GitResult<Self> {
        let mut app = Self::base(Some(git::Repo::open(path)?));
        app.refresh();
        Ok(app)
    }

    /// Repo-free instance backed by `mock` data, for the render tests.
    pub fn mock() -> Self {
        let mut app = Self::base(None);
        app.header = mock::mock_header();
        app.files = mock::mock_files();
        app.update_preview();
        app
    }

    /// Query the real terminal for a graphics protocol and, if it has one,
    /// swap it in for the half-block fallback. Call once, before `run`.
    pub fn detect_graphics(&mut self) {
        if let Ok(picker) = Picker::from_query_stdio() {
            self.picker = picker;
            self.update_preview();
        }
    }

    /// Re-read the wired panes. On error keep the old snapshot and stash the
    /// message; never propagate, never panic. No-op without a repo.
    pub fn refresh(&mut self) {
        let Some(repo) = &self.repo else { return };
        match repo.snapshot() {
            Ok(snap) => {
                self.header = snap.header;
                self.files = snap.files;
                self.last_error = None;
            }
            Err(e) => self.last_error = Some(e.to_string()),
        }
        let last = self.row_count(Pane::Files).saturating_sub(1);
        let cursor = &mut self.selection[Pane::Files.index()];
        *cursor = (*cursor).min(last);
        self.update_preview();
    }

    /// Rebuild `preview` for the current focus and selection. Cheap when the
    /// selection is not an image (the common case); decodes otherwise.
    fn update_preview(&mut self) {
        self.preview = self.build_preview();
    }

    fn build_preview(&self) -> Preview {
        if self.focus != Pane::Files {
            return Preview::None;
        }
        let Some(entry) = self.files.get(self.selected(Pane::Files)) else {
            return Preview::None;
        };
        if !preview::is_image_path(&entry.path) {
            return Preview::None;
        }
        let bytes = match &self.repo {
            Some(repo) => match repo.blob_bytes(&entry.path, git::Rev::Workdir) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Preview::Note(format!("[image] {}  ({e})", entry.path.display()));
                }
            },
            None => mock::mock_image_bytes(&entry.path)
                .map(<[u8]>::to_vec)
                .unwrap_or_default(),
        };
        preview::from_bytes(&self.picker, &entry.path, &bytes)
    }

    /// The right-pane preview for the current selection.
    pub fn preview(&self) -> &Preview {
        &self.preview
    }

    /// Focus `pane` and move its cursor to `index`, rebuilding the preview.
    /// Test and example helper; the running app goes through `on_key`.
    pub fn select(&mut self, pane: Pane, index: usize) {
        self.focus = pane;
        let last = self.row_count(pane).saturating_sub(1);
        self.selection[pane.index()] = index.min(last);
        self.update_preview();
    }

    /// Selection cursor for a given pane.
    pub fn selected(&self, pane: Pane) -> usize {
        self.selection[pane.index()]
    }

    /// Selectable row count for a pane, for clamping the cursor and deciding
    /// whether to draw a highlight.
    pub fn row_count(&self, pane: Pane) -> usize {
        match pane {
            Pane::Status => 0,
            Pane::Files => self.files.len(),
            Pane::Branches => mock::BRANCHES.len(),
            Pane::Commits => mock::COMMITS.len(),
            Pane::Stash => mock::STASH.len(),
        }
    }

    /// Status pane rows: the header summary, or the error when `refresh()`
    /// failed.
    pub fn status_lines(&self) -> Vec<Line<'static>> {
        if let Some(err) = &self.last_error {
            return vec![theme::error_line(&format!("error: {err}"))];
        }
        let h = &self.header;
        let mut first = h.branch.clone();
        if let Some(up) = &h.upstream {
            first.push_str(&format!(" \u{2192} {up}"));
        }
        if h.ahead > 0 {
            first.push_str(&format!(" \u{2191}{}", h.ahead));
        }
        if h.behind > 0 {
            first.push_str(&format!(" \u{2193}{}", h.behind));
        }
        let second = if h.conflicts > 0 {
            format!("\u{2717} {} merge conflict(s)", h.conflicts)
        } else {
            "\u{2713} no merge conflicts".to_string()
        };
        vec![theme::status_line(&first), theme::status_line(&second)]
    }

    /// Files pane rows, or a single "working tree clean" line.
    pub fn file_lines(&self) -> Vec<Line<'static>> {
        if self.files.is_empty() {
            return vec![Line::raw("working tree clean")];
        }
        self.files
            .iter()
            .map(|f| theme::file_line(&f.display()))
            .collect()
    }

    /// Draw, then block on one event, until `should_quit`. No tick, no polling.
    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| ui::draw(frame, self))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn handle_events(&mut self) -> Result<()> {
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            self.on_key(key);
        }
        Ok(())
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        if self.show_help {
            if matches!(
                key.code,
                KeyCode::Char('?') | KeyCode::Char('q') | KeyCode::Esc
            ) {
                self.show_help = false;
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char(c @ '1'..='5') => {
                self.focus = PANES[c as usize - '1' as usize];
            }
            KeyCode::Tab | KeyCode::Right => self.focus = self.pane_offset(1),
            KeyCode::BackTab | KeyCode::Left => self.focus = self.pane_offset(PANES.len() - 1),
            KeyCode::Char('j') | KeyCode::Down => self.select_down(),
            KeyCode::Char('k') | KeyCode::Up => self.select_up(),
            _ => {}
        }

        // Focus or selection may have moved; keep the right-pane preview in sync.
        self.update_preview();
    }

    fn pane_offset(&self, delta: usize) -> Pane {
        PANES[(self.focus.index() + delta) % PANES.len()]
    }

    fn select_down(&mut self) {
        let last = self.row_count(self.focus).saturating_sub(1);
        let cursor = &mut self.selection[self.focus.index()];
        *cursor = (*cursor + 1).min(last);
    }

    fn select_up(&mut self) {
        let cursor = &mut self.selection[self.focus.index()];
        *cursor = cursor.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyCode;

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::from(code));
    }

    #[test]
    fn arrows_cycle_panes_and_wrap() {
        let mut app = App::mock();
        press(&mut app, KeyCode::Right);
        assert_eq!(app.focus, Pane::Files);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.focus, Pane::Status);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.focus, Pane::Stash, "Left from the first pane wraps");
    }

    #[test]
    fn selection_clamps_at_both_ends() {
        let mut app = App::mock();
        let last = mock::mock_files().len() - 1;
        press(&mut app, KeyCode::Char('2')); // focus Files
        for _ in 0..20 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.selected(Pane::Files), last);
        for _ in 0..20 {
            press(&mut app, KeyCode::Up);
        }
        assert_eq!(app.selected(Pane::Files), 0);
    }

    #[test]
    fn image_selection_builds_an_image_preview() {
        let mut app = App::mock();
        let png = mock::mock_files()
            .iter()
            .position(|f| f.path.extension().is_some_and(|e| e == "png"))
            .expect("mock has a .png entry");

        press(&mut app, KeyCode::Char('2')); // focus Files
        assert!(matches!(app.preview(), Preview::None), "src/main.rs is not an image");

        for _ in 0..png {
            press(&mut app, KeyCode::Down);
        }
        assert!(
            matches!(app.preview(), Preview::Image(_)),
            "the embedded PNG decodes on the half-block picker"
        );

        press(&mut app, KeyCode::Char('1')); // leave Files
        assert!(matches!(app.preview(), Preview::None));
    }

    #[test]
    fn help_overlay_swallows_navigation() {
        let mut app = App::mock();
        press(&mut app, KeyCode::Char('?'));
        assert!(app.show_help);
        press(&mut app, KeyCode::Right);
        assert_eq!(app.focus, Pane::Status, "nav is inert while help is up");
        press(&mut app, KeyCode::Char('?'));
        assert!(!app.show_help);
    }

    #[test]
    fn refresh_without_repo_is_a_noop() {
        let mut app = App::mock();
        let before = app.file_lines().len();
        app.refresh();
        assert_eq!(app.file_lines().len(), before);
        assert_eq!(before, mock::mock_files().len());
    }
}
