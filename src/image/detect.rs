//! Which `Picker` (graphics protocol) to render images with.
//!
//! See `docs/BUG_IMAGE_PREVIEW.md`: some terminal hosts answer the capability
//! query with a protocol they then don't actually draw, so a query success is
//! not proof the picture will show up. The half-block fallback (`Picker::
//! halfblocks`) always draws, everywhere, but its resolution is the character
//! grid: in a small pane the picture turns chunky. A native protocol
//! (kitty / iTerm2 / sixel) draws at the cell's pixel size instead, so it
//! stays sharp even when the terminal is small.
//!
//! Environment overrides, all read once here:
//! - `FERRIT_NO_GRAPHICS`    force half-blocks, skip everything else.
//! - `FERRIT_FORCE_GRAPHICS` run the capability query even on a host we would
//!   otherwise pin to a fixed protocol (zellij, VS Code).
//! - `FERRIT_GRAPHICS=<p>`   force protocol `<p>` (one of `halfblocks`,
//!   `sixel`, `kitty`, `iterm2`), overriding detection. Use when the picked
//!   protocol renders blank on some host.

use ratatui_image::picker::{Picker, ProtocolType};

/// Query the real terminal for a graphics protocol. Returns `None` when the
/// query fails, or when the host is known to lie about support, so the
/// caller keeps its half-block fallback.
///
/// Call once, before entering the alternate screen, same as the ratatui-image
/// examples: `Picker::from_query_stdio` reads and writes stdio momentarily.
pub fn detect_picker() -> Option<Picker> {
    if std::env::var_os("FERRIT_NO_GRAPHICS").is_some() {
        return None;
    }
    let forced = protocol_override();

    // Hosts where the capability query is useless or misleading: skip it (also
    // saves a 2s stdio timeout), start from a plain picker, and pin the
    // protocol they actually render.
    if let Some(proto) = pinned_protocol() {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(forced.unwrap_or(proto));
        return Some(picker);
    }

    let mut picker = Picker::from_query_stdio().ok()?;
    if is_iterm2() && picker.protocol_type() == ProtocolType::Kitty {
        // iTerm2 (>= 3.5) answers the kitty graphics query but only draws part
        // of the protocol; ratatui-image's kitty encoder uses unicode
        // placeholders it never renders, so the pane comes out blank. iTerm2's
        // own inline-image protocol works, so prefer it.
        picker.set_protocol_type(ProtocolType::Iterm2);
    }
    if let Some(proto) = forced {
        picker.set_protocol_type(proto);
    }
    Some(picker)
}

/// Protocol pinned for the current host without asking it, or `None` to run
/// the capability query. `FERRIT_FORCE_GRAPHICS` forces the query path.
///
/// - **zellij** (>= 0.40) renders sixel in its own pane compositor but has no
///   passthrough for kitty / iTerm2 graphics.
/// - **VS Code** integrated terminal renders sixel (and iTerm2) once
///   `terminal.integrated.enableImages` is on (see `.vscode/settings.json`),
///   but `from_query_stdio` there reports iTerm2 and then drops every frame.
///
/// Old zellij without sixel, or VS Code with the setting off, still come out
/// blank: `FERRIT_GRAPHICS=halfblocks` (or `FERRIT_NO_GRAPHICS`) falls back.
fn pinned_protocol() -> Option<ProtocolType> {
    if std::env::var_os("FERRIT_FORCE_GRAPHICS").is_some() {
        return None;
    }
    if std::env::var_os("ZELLIJ").is_some() || is_vscode() {
        return Some(ProtocolType::Sixel);
    }
    None
}

/// Real iTerm2, including through ssh / tmux where it exports `LC_TERMINAL`.
fn is_iterm2() -> bool {
    std::env::var_os("TERM_PROGRAM").as_deref() == Some(std::ffi::OsStr::new("iTerm.app"))
        || std::env::var_os("LC_TERMINAL").as_deref() == Some(std::ffi::OsStr::new("iTerm2"))
}

/// A one-line report of what `detect_picker` settled on, for the Status pane
/// when `FERRIT_DEBUG` is set. `None` means the half-block fallback is in use.
pub fn debug_line(picker: &Picker) -> Option<String> {
    std::env::var_os("FERRIT_DEBUG")?;
    let fs = picker.font_size();
    Some(format!(
        "graphics: {:?}  font {}x{}  (FERRIT_GRAPHICS to override)",
        picker.protocol_type(),
        fs.width,
        fs.height,
    ))
}

/// `FERRIT_GRAPHICS` parsed into a `ProtocolType`, if set to a known value.
fn protocol_override() -> Option<ProtocolType> {
    match std::env::var("FERRIT_GRAPHICS").ok()?.to_ascii_lowercase().as_str() {
        "halfblocks" | "halfblock" | "hb" => Some(ProtocolType::Halfblocks),
        "sixel" => Some(ProtocolType::Sixel),
        "kitty" => Some(ProtocolType::Kitty),
        "iterm2" | "iterm" => Some(ProtocolType::Iterm2),
        _ => None,
    }
}

fn is_vscode() -> bool {
    std::env::var_os("TERM_PROGRAM").as_deref() == Some(std::ffi::OsStr::new("vscode"))
}
