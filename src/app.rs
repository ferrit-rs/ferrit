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

use crate::git::{self, GitResult};
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

#[derive(Default)]
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
}

impl App {
    /// Open the repo at or above `path`, then take one snapshot.
    pub fn open(path: &Path) -> GitResult<Self> {
        let repo = git::Repo::open(path)?;
        let mut app = Self {
            repo: Some(repo),
            ..Self::default()
        };
        app.refresh();
        Ok(app)
    }

    /// Repo-free instance backed by `mock` data, for the render tests.
    pub fn mock() -> Self {
        Self {
            header: mock::mock_header(),
            files: mock::mock_files(),
            ..Self::default()
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
        press(&mut app, KeyCode::Char('2')); // Files: 3 rows
        for _ in 0..10 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.selected(Pane::Files), 2);
        for _ in 0..10 {
            press(&mut app, KeyCode::Up);
        }
        assert_eq!(app.selected(Pane::Files), 0);
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
        app.refresh();
        assert_eq!(app.file_lines().len(), 3);
    }
}
