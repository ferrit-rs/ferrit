use std::io::{self, Write};

/// Sets the terminal's native mouse pointer shape for hoverable controls.
/// Terminals without OSC 22 support ignore these sequences.
#[derive(Debug, Default)]
pub struct MousePointer {
    is_hand: bool,
}

impl MousePointer {
    /// Show a pointing hand while `hovered`, restoring the terminal default
    /// when the pointer leaves the interactive target.
    pub fn set_hovered(&mut self, hovered: bool) -> io::Result<()> {
        if self.is_hand == hovered {
            return Ok(());
        }
        Self::write_shape(hovered)?;
        self.is_hand = hovered;
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
