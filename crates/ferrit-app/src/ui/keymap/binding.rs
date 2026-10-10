//! Key and modifier parsing for config and terminal events.

use std::fmt;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A key and the modifiers that matter. `Shift` is represented by a
/// character's case, and `BackTab` is its own key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
}

impl KeyBinding {
    pub fn of(code: KeyCode) -> Self {
        Self::new(code, false, false)
    }

    fn new(code: KeyCode, ctrl: bool, alt: bool) -> Self {
        let code = match code {
            KeyCode::Char(c) if ctrl || alt => KeyCode::Char(c.to_ascii_lowercase()),
            other => other,
        };
        Self { code, ctrl, alt }
    }

    /// Binding a terminal key event triggers.
    pub fn from_event(event: KeyEvent) -> Self {
        Self::new(
            event.code,
            event.modifiers.contains(KeyModifiers::CONTROL),
            event.modifiers.contains(KeyModifiers::ALT),
        )
    }

    /// Parse `q`, named keys, or `ctrl-d`/`alt-x`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut rest = text;
        let (mut ctrl, mut alt) = (false, false);
        loop {
            let lower = rest.to_ascii_lowercase();
            if let Some(tail) = lower.strip_prefix("ctrl-") {
                ctrl = true;
                rest = rest.get(rest.len() - tail.len()..)?;
            } else if let Some(tail) = lower.strip_prefix("alt-") {
                alt = true;
                rest = rest.get(rest.len() - tail.len()..)?;
            } else {
                break;
            }
        }
        let mut chars = rest.chars();
        let code = match (chars.next(), chars.next()) {
            (Some(c), None) => KeyCode::Char(c),
            _ => named_key(&rest.to_ascii_lowercase())?,
        };
        Some(Self::new(code, ctrl, alt))
    }
}

fn named_key(name: &str) -> Option<KeyCode> {
    Some(match name {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "tab" => KeyCode::Tab,
        "backtab" | "shift-tab" => KeyCode::BackTab,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "pgup" | "pageup" => KeyCode::PageUp,
        "pgdn" | "pagedown" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        other => {
            let number: u8 = other.strip_prefix('f')?.parse().ok()?;
            if !(1..=12).contains(&number) {
                return None;
            }
            KeyCode::F(number)
        },
    })
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("ctrl-")?;
        }
        if self.alt {
            f.write_str("alt-")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::Enter => f.write_str("enter"),
            KeyCode::Esc => f.write_str("esc"),
            KeyCode::Tab => f.write_str("tab"),
            KeyCode::BackTab => f.write_str("backtab"),
            KeyCode::Up => f.write_str("up"),
            KeyCode::Down => f.write_str("down"),
            KeyCode::Left => f.write_str("left"),
            KeyCode::Right => f.write_str("right"),
            KeyCode::PageUp => f.write_str("pgup"),
            KeyCode::PageDown => f.write_str("pgdn"),
            KeyCode::Home => f.write_str("home"),
            KeyCode::End => f.write_str("end"),
            KeyCode::Backspace => f.write_str("backspace"),
            KeyCode::Delete => f.write_str("delete"),
            KeyCode::Insert => f.write_str("insert"),
            KeyCode::F(n) => write!(f, "f{n}"),
            _ => f.write_str("?"),
        }
    }
}
