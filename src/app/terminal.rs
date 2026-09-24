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

use crate::components::ui::mouse_pointer::MousePointer;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Enable raw mode, switch to the alternate screen, hide the cursor, and, when
/// `mouse` is set, capture the mouse (wheel scroll, clicks, hover). `mouse` is
/// `[ui] mouse`: off leaves the terminal's own text selection working.
pub fn init(mouse: bool) -> io::Result<Tui> {
    enable_raw_mode()?;
    let entered = execute!(io::stdout(), EnterAlternateScreen, Hide).and_then(|()| {
        if mouse {
            execute!(io::stdout(), EnableMouseCapture)
        } else {
            Ok(())
        }
    });
    if let Err(error) = entered {
        let _ = restore();
        return Err(error);
    }
    if let Err(error) = MousePointer::reset_terminal() {
        let _ = restore();
        return Err(error);
    }
    set_panic_hook();
    match Terminal::new(CrosstermBackend::new(io::stdout())) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let _ = restore();
            Err(error)
        },
    }
}

/// Exact reverse of `init`. Safe to call more than once.
pub fn restore() -> io::Result<()> {
    let pointer = MousePointer::reset_terminal();
    let screen = execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        Show
    );
    let raw = disable_raw_mode();
    pointer.and(screen).and(raw)
}

fn set_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        hook(info);
    }));
}
