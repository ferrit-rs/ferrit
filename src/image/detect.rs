//! Which `Picker` (graphics protocol) to render images with.
//!
//! See `docs/BUG_IMAGE_PREVIEW.md`. A query success is not proof the picture
//! shows up: some hosts answer `Picker::from_query_stdio` with a protocol they
//! then never draw. The half-block fallback (`Picker::halfblocks`) draws
//! everywhere but at character-grid resolution, so a small pane turns chunky;
//! a native protocol (kitty / iTerm2 / sixel) draws at the cell's pixel size
//! and stays sharp.
//!
//! Two inputs decide the protocol:
//!
//! - the [`Host`], classified once from the environment. iTerm2, VS Code and
//!   zellij each mis-answer the query in their own way, so each carries its own
//!   [`Plan`].
//! - the [`Override`] knobs, also read once, layered on top of the host rule:
//!   - `FERRIT_NO_GRAPHICS`    keep half-blocks, skip everything.
//!   - `FERRIT_FORCE_GRAPHICS` run the query even on a host we would pin.
//!   - `FERRIT_GRAPHICS=<p>`   force `<p>` (`halfblocks`, `sixel`, `kitty`,
//!     `iterm2`); final say, overrides every rule below.

use ratatui_image::picker::{Picker, ProtocolType};

/// Outcome of [`pick`]: the `Picker` to render with, plus the context that
/// produced it so `FERRIT_DEBUG` can report it.
pub struct Detected {
    pub picker: Picker,
    host: Host,
    forced: Option<ProtocolType>,
}

/// Query the terminal and settle on a graphics protocol. `None` means keep the
/// caller's half-block fallback: either `FERRIT_NO_GRAPHICS`, or the capability
/// query failed on a host that needs one.
///
/// Call once, before entering the alternate screen: `from_query_stdio` reads
/// and writes stdio for a moment, same as the ratatui-image examples.
pub fn pick() -> Option<Detected> {
    let over = Override::read();
    if over.disabled {
        return None;
    }

    let host = Host::detect();
    let plan = if over.force_query {
        Plan::Query
    } else {
        host.plan()
    };

    let mut picker = match plan {
        Plan::Pin(proto) => {
            // Skip the query (also its ~2s stdio timeout on a mute host), start
            // from a plain picker, force the protocol this host really draws.
            let mut p = Picker::halfblocks();
            p.set_protocol_type(proto);
            p
        },
        Plan::Query => Picker::from_query_stdio().ok()?,
        Plan::QueryOr { when, swap_to } => {
            let mut p = Picker::from_query_stdio().ok()?;
            if p.protocol_type() == when {
                p.set_protocol_type(swap_to);
            }
            p
        },
    };

    if let Some(proto) = over.protocol {
        picker.set_protocol_type(proto);
    }
    Some(Detected {
        picker,
        host,
        forced: over.protocol,
    })
}

impl Detected {
    /// One-line report of what [`pick`] settled on, for the Status pane when
    /// `FERRIT_DEBUG` is set. `None` when it is not.
    pub fn debug_line(&self) -> Option<String> {
        std::env::var_os("FERRIT_DEBUG")?;
        let fs = self.picker.font_size();
        let tail = if self.forced.is_some() {
            "  (FERRIT_GRAPHICS)"
        } else {
            "  (FERRIT_GRAPHICS to override)"
        };
        Some(format!(
            "graphics: {:?} on {:?}  font {}x{}{tail}",
            self.picker.protocol_type(),
            self.host,
            fs.width,
            fs.height,
        ))
    }
}

/// How to get a protocol for a given [`Host`].
enum Plan {
    /// Trust whatever `from_query_stdio` answers.
    Query,
    /// Run the query, but if it answers `when`, use `swap_to` instead.
    QueryOr {
        when: ProtocolType,
        swap_to: ProtocolType,
    },
    /// Skip the query, this host lies about it: render this protocol.
    Pin(ProtocolType),
}

/// The terminal host, as far as graphics support goes. Classified once from the
/// environment; the order of the checks in [`Host::detect`] is their precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Host {
    /// Real iTerm2, including through ssh / tmux where it exports `LC_TERMINAL`.
    Iterm2,
    /// VS Code integrated terminal.
    Vscode,
    /// A zellij pane.
    Zellij,
    /// Anything else: the capability query is trusted as-is.
    Other,
}

impl Host {
    fn detect() -> Self {
        let is = |key, val| std::env::var_os(key).as_deref() == Some(std::ffi::OsStr::new(val));
        if is("TERM_PROGRAM", "iTerm.app") || is("LC_TERMINAL", "iTerm2") {
            Self::Iterm2
        } else if is("TERM_PROGRAM", "vscode") {
            Self::Vscode
        } else if std::env::var_os("ZELLIJ").is_some() {
            Self::Zellij
        } else {
            Self::Other
        }
    }

    fn plan(self) -> Plan {
        match self {
            // iTerm2 (>= 3.5) answers the kitty graphics query, so the query
            // returns `Kitty`; it only half-implements the protocol and
            // ratatui-image's kitty encoder needs the unicode-placeholder part
            // it never draws. iTerm2's own inline-image protocol works.
            Self::Iterm2 => Plan::QueryOr {
                when: ProtocolType::Kitty,
                swap_to: ProtocolType::Iterm2,
            },
            // Both answer the query with a protocol they then drop every frame
            // of, and both composite sixel themselves: zellij (>= 0.40)
            // natively, VS Code once `terminal.integrated.enableImages` is on
            // (see `.vscode/settings.json`).
            Self::Vscode | Self::Zellij => Plan::Pin(ProtocolType::Sixel),
            Self::Other => Plan::Query,
        }
    }
}

/// The `FERRIT_*` graphics knobs, read from the environment exactly once.
struct Override {
    /// `FERRIT_NO_GRAPHICS`: keep the half-block fallback, detect nothing.
    disabled: bool,
    /// `FERRIT_FORCE_GRAPHICS`: run the capability query even on a pinned host.
    force_query: bool,
    /// `FERRIT_GRAPHICS=<p>`: the last word on the protocol, if set and valid.
    protocol: Option<ProtocolType>,
}

impl Override {
    fn read() -> Self {
        Self {
            disabled: std::env::var_os("FERRIT_NO_GRAPHICS").is_some(),
            force_query: std::env::var_os("FERRIT_FORCE_GRAPHICS").is_some(),
            protocol: std::env::var("FERRIT_GRAPHICS")
                .ok()
                .and_then(|v| parse_protocol(&v)),
        }
    }
}

/// `FERRIT_GRAPHICS` value parsed into a `ProtocolType`, if it names one.
fn parse_protocol(s: &str) -> Option<ProtocolType> {
    match s.to_ascii_lowercase().as_str() {
        "halfblocks" | "halfblock" | "hb" => Some(ProtocolType::Halfblocks),
        "sixel" => Some(ProtocolType::Sixel),
        "kitty" => Some(ProtocolType::Kitty),
        "iterm2" | "iterm" => Some(ProtocolType::Iterm2),
        _ => None,
    }
}
