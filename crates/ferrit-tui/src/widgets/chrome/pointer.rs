//! Terminal mouse pointer shape.

use std::io::{self, Write};

/// Sets the terminal's native mouse pointer shape for hoverable controls.
#[derive(Debug, Default)]
pub struct MousePointer {
    wanted: bool,
    is_hand: bool,
}

impl MousePointer {
    /// Record whether the pointer is over an interactive target.
    pub fn request(&mut self, hovered: bool) {
        self.wanted = hovered;
    }

    /// Show a pointing hand while requested, restoring the terminal default
    /// when the pointer leaves the target. Writes only on a change.
    pub fn sync(&mut self) -> io::Result<()> {
        if self.is_hand == self.wanted {
            return Ok(());
        }
        Self::write_shape(self.wanted)?;
        self.is_hand = self.wanted;
        Ok(())
    }

    /// Reset OSC 22 state when Ferrit starts or restores the terminal.
    pub fn reset_terminal() -> io::Result<()> {
        Self::write_shape(false)
    }

    fn write_shape(hand: bool) -> io::Result<()> {
        let sequence = if hand {
            b"\x1b]22;pointer\x1b\\".as_slice()
        } else {
            b"\x1b]22;\x1b\\".as_slice()
        };
        let mut stdout = io::stdout().lock();
        stdout.write_all(sequence)?;
        stdout.flush()
    }
}
