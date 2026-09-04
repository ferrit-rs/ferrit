//! Application state and the draw / event loop.
//!
//! Phase 1 is layout only: no git, no mock panes yet. This is the M0 skeleton,
//! a single centered placeholder, that later milestones build the real screen on.

use color_eyre::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Alignment;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::Tui;

#[derive(Default)]
pub struct App {
    should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    /// Draw, then block on one event, until `should_quit`.
    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let body = Paragraph::new("ferrit\n\nphase 1 skeleton, press q to quit")
            .alignment(Alignment::Center)
            .block(Block::default().title(" ferrit ").borders(Borders::ALL));
        frame.render_widget(body, frame.area());
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
        match (key.modifiers, key.code) {
            (_, KeyCode::Char('q')) => self.should_quit = true,
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => self.should_quit = true,
            _ => {}
        }
    }
}
