//! Terminal plumbing: enter/leave the alternate screen and keep it recoverable.
//!
//! `init` is the exact reverse of `restore`. A panic hook also calls `restore`
//! so a crash never leaves the user's terminal in raw mode.

use std::io::{self, Stdout};

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Enable raw mode, switch to the alternate screen, capture the mouse (for
/// wheel scroll of the diff pane), hide the cursor.
pub fn init() -> io::Result<Tui> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture, Hide)?;
    set_panic_hook();
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

/// Exact reverse of `init`. Safe to call more than once.
pub fn restore() -> io::Result<()> {
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        Show
    )?;
    disable_raw_mode()
}

fn set_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        hook(info);
    }));
}
