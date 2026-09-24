use std::io::{self, Write};

/// Sets the terminal's native mouse pointer shape for hoverable controls.
/// Terminals without OSC 22 support ignore these sequences.
#[derive(Debug, Default)]
pub struct MousePointer {
    /// What the hover logic last asked for.
    wanted: bool,
    /// What the terminal is currently showing.
    is_hand: bool,
}

impl MousePointer {
    /// Record whether the pointer is over an interactive target. Nothing is
    /// written until `sync`.
    pub fn request(&mut self, hovered: bool) {
        self.wanted = hovered;
    }

    /// Show a pointing hand while the last `request` was `true`, restoring
    /// the terminal default when the pointer leaves the target. Writes only
    /// on a change.
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
