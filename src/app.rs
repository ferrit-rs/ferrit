//! Application state and the draw / event loop.
//!
//! Phase 1 is navigation only: `App` owns which left pane is focused and one
//! selection cursor per pane. All displayed data lives in `mock`. There is no
//! git anywhere.

use color_eyre::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::mock;
use crate::tui::Tui;
use crate::ui;

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

    /// The mock rows this pane lists.
    pub fn items(self) -> &'static [&'static str] {
        match self {
            Pane::Status => mock::STATUS,
            Pane::Files => mock::FILES,
            Pane::Branches => mock::BRANCHES,
            Pane::Commits => mock::COMMITS,
            Pane::Stash => mock::STASH,
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
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    /// Selection cursor for a given pane.
    pub fn selected(&self, pane: Pane) -> usize {
        self.selection[pane.index()]
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
            KeyCode::Char(c @ '1'..='5') => {
                self.focus = PANES[c as usize - '1' as usize];
            }
            KeyCode::Tab => self.focus = self.pane_offset(1),
            KeyCode::BackTab => self.focus = self.pane_offset(PANES.len() - 1),
            KeyCode::Char('j') | KeyCode::Down => self.select_down(),
            KeyCode::Char('k') | KeyCode::Up => self.select_up(),
            _ => {}
        }
    }

    fn pane_offset(&self, delta: usize) -> Pane {
        PANES[(self.focus.index() + delta) % PANES.len()]
    }

    fn select_down(&mut self) {
        let last = self.focus.items().len().saturating_sub(1);
        let cursor = &mut self.selection[self.focus.index()];
        *cursor = (*cursor + 1).min(last);
    }

    fn select_up(&mut self) {
        let cursor = &mut self.selection[self.focus.index()];
        *cursor = cursor.saturating_sub(1);
    }
}
