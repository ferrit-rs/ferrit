//! Colour palette and the span builders that give each kind of line its
//! meaning-carrying colour. Phase 1 keeps this to one flat palette: no config,
//! no themes, just enough colour to tell the panes and the diff apart.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

/// Border of the focused left pane.
pub const FOCUS: Color = Color::Cyan;
/// Border of every unfocused pane and other low-priority chrome.
pub const IDLE: Color = Color::DarkGray;
/// Added diff line, checked-out branch.
pub const ADD: Color = Color::Green;
/// Removed diff line, deleted path.
pub const DEL: Color = Color::Red;
/// Hunk header (`@@ ... @@`).
pub const HUNK: Color = Color::Cyan;
/// Commit hash.
pub const HASH: Color = Color::Yellow;
/// Modified path, ahead/behind counts.
pub const WARN: Color = Color::Yellow;
/// Key names in the keybind bar.
pub const KEY: Color = Color::Magenta;

fn fg(color: Color) -> Style {
    Style::new().fg(color)
}

/// `<status> <path>` from `git status --porcelain`: colour the two-char code by
/// what it means, leave the path plain.
pub fn file_line(raw: &str) -> Line<'static> {
    let (code, rest) = raw.split_at(raw.len().min(2));
    let color = if code.contains('D') {
        DEL
    } else if code.contains('?') {
        IDLE
    } else if code.contains('A') {
        ADD
    } else {
        WARN
    };
    Line::from(vec![
        Span::styled(code.to_string(), fg(color)),
        Span::raw(rest.to_string()),
    ])
}

/// `* main` gets green + bold, the rest stay plain.
pub fn branch_line(raw: &'static str) -> Line<'static> {
    if let Some(name) = raw.strip_prefix("* ") {
        Line::from(vec![
            Span::styled("* ", fg(ADD)),
            Span::styled(name, Style::new().fg(ADD).add_modifier(Modifier::BOLD)),
        ])
    } else {
        Line::raw(raw)
    }
}

/// `<hash> <subject>`: hash in yellow, subject plain.
pub fn commit_line(raw: &'static str) -> Line<'static> {
    match raw.split_once(' ') {
        Some((hash, subject)) => Line::from(vec![
            Span::styled(hash, fg(HASH)),
            Span::raw(" "),
            Span::raw(subject),
        ]),
        None => Line::raw(raw),
    }
}

/// A `git diff` blob, coloured line by line.
pub fn diff_text(raw: &'static str) -> Text<'static> {
    let lines = raw.lines().map(|line| {
        let style = if line.starts_with("@@") {
            fg(HUNK)
        } else if line.starts_with("diff --git")
            || line.starts_with("index ")
            || line.starts_with("commit ")
            || line.starts_with("Author:")
            || line.starts_with("Date:")
        {
            Style::new().fg(IDLE).add_modifier(Modifier::BOLD)
        } else if line.starts_with('+') {
            fg(ADD)
        } else if line.starts_with('-') {
            fg(DEL)
        } else {
            Style::new()
        };
        Line::styled(line, style)
    });
    Text::from(lines.collect::<Vec<_>>())
}

/// Repo status header line: highlight the ahead/behind arrows.
pub fn status_line(raw: &str) -> Line<'static> {
    let style = if raw.contains('\u{2191}') || raw.contains('\u{2193}') {
        fg(WARN)
    } else {
        fg(ADD)
    };
    Line::styled(raw.to_string(), style)
}

/// A `refresh()` failure, surfaced in the Status pane instead of a panic.
pub fn error_line(raw: &str) -> Line<'static> {
    Line::styled(raw.to_string(), fg(DEL))
}

/// Command-log line: dim the `$` prompt, leave the command bright.
pub fn log_line(raw: &'static str) -> Line<'static> {
    match raw.strip_prefix("$ ") {
        Some(cmd) => Line::from(vec![Span::styled("$ ", fg(IDLE)), Span::raw(cmd)]),
        None => Line::styled(raw, fg(IDLE)),
    }
}

/// Keybind bar: `<key>` tokens in magenta, everything else dim.
pub fn keybar_line(raw: &'static str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut rest = raw;
    while let Some(open) = rest.find('<') {
        if open > 0 {
            spans.push(Span::styled(&rest[..open], fg(IDLE)));
        }
        rest = &rest[open..];
        match rest.find('>') {
            Some(close) => {
                spans.push(Span::styled(&rest[..=close], fg(KEY)));
                rest = &rest[close + 1..];
            }
            None => break,
        }
    }
    if !rest.is_empty() {
        spans.push(Span::styled(rest, fg(IDLE)));
    }
    Line::from(spans)
}
