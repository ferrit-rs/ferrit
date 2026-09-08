//! Colour palette and the span builders that give each kind of line its
//! meaning-carrying colour. One flat palette tuned to match lazygit's default
//! theme: green for the focused pane, a solid blue selection bar, green hashes,
//! yellow keys. No config, no theme switching yet (that is phase 10).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};

use crate::git::{BranchEntry, CommitEntry, Diff, FileEntry, StashEntry};

/// Border and title of the focused left pane (lazygit `activeBorderColor`).
pub const FOCUS: Color = Color::Green;
/// Border of every unfocused pane and other low-priority chrome
/// (lazygit `inactiveBorderColor`, roughly the default foreground).
pub const IDLE: Color = Color::Gray;
/// Background of the selected row (lazygit `selectedLineBgColor`).
pub const SELECTION: Color = Color::Blue;
/// Text on the selected row.
pub const SELECTION_FG: Color = Color::White;
/// Added diff line, checked-out branch.
pub const ADD: Color = Color::Green;
/// Removed diff line, deleted path.
pub const DEL: Color = Color::Red;
/// Hunk header (`@@ ... @@`).
pub const HUNK: Color = Color::Cyan;
/// Commit hash and the graph node.
pub const HASH: Color = Color::Green;
/// Author initials in the commit list.
pub const AUTHOR: Color = Color::Magenta;
/// Modified path, ahead/behind counts, the `N of M` counter.
pub const WARN: Color = Color::Yellow;
/// Key names in the keybind bar.
pub const KEY: Color = Color::Yellow;

fn fg(color: Color) -> Style {
    Style::new().fg(color)
}

/// Style for the selected row in a left-pane list. Like lazygit: a solid blue
/// bar only in the focused pane, filled across the pane width by the `List`
/// widget. Unfocused panes keep a cursor position but draw no bar, so only one
/// selection reads as "live" at a time.
pub fn selection_style(focused: bool) -> Style {
    if focused {
        Style::new()
            .bg(SELECTION)
            .fg(SELECTION_FG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    }
}

/// Bottom-right `N of M` counter shown on each list pane's border.
pub fn counter_line(current: usize, total: usize) -> Line<'static> {
    Line::styled(format!(" {current} of {total} "), fg(IDLE)).right_aligned()
}

/// Files row, porcelain layout `XY path` (X = staged, Y = worktree): colour the
/// two-char code by what it means, leave the path plain.
pub fn file_line(entry: &FileEntry) -> Line<'static> {
    let code = format!("{}{}", entry.staged.code(), entry.worktree.code());
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
        Span::styled(code, fg(color)),
        Span::raw(format!(" {}", entry.path.display())),
    ])
}

/// lazygit branch row: `* main ↑2` for the checked-out branch (green, bold),
/// `  feat/x` for the rest. Ahead/behind arrows in yellow when there is an
/// upstream to compare against.
pub fn branch_line(entry: &BranchEntry) -> Line<'static> {
    let marker = if entry.is_head { "* " } else { "  " };
    let name_style = if entry.is_head {
        Style::new().fg(ADD).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    let mut spans = vec![
        Span::styled(marker, fg(ADD)),
        Span::styled(entry.name.clone(), name_style),
    ];
    if entry.upstream.is_some() {
        if entry.ahead > 0 {
            spans.push(Span::styled(format!(" \u{2191}{}", entry.ahead), fg(WARN)));
        }
        if entry.behind > 0 {
            spans.push(Span::styled(format!(" \u{2193}{}", entry.behind), fg(WARN)));
        }
    }
    Line::from(spans)
}

/// lazygit commit row: `<hash> <initials> <graph-node> <subject>`. Hash and
/// node in green, author initials in magenta, subject plain. `graph` is the
/// graph-column glyph, a plain `o` for linear history until phase 2 G4.
pub fn commit_line(entry: &CommitEntry) -> Line<'static> {
    Line::from(vec![
        Span::styled(entry.short_hash.clone(), fg(HASH)),
        Span::raw(" "),
        Span::styled(entry.author_initials(), fg(AUTHOR)),
        Span::raw(" "),
        Span::styled("o", fg(HASH)),
        Span::raw(" "),
        Span::raw(entry.summary.clone()),
    ])
}

/// lazygit stash row: `stash@{0}: message`.
pub fn stash_line(entry: &StashEntry) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("stash@{{{}}}", entry.index), fg(HASH)),
        Span::raw(": "),
        Span::raw(entry.message.clone()),
    ])
}

/// Colour of one diff line, by its leading bytes. Matches what `git --color`
/// paints: hunk header cyan, `+`/`-` green/red, file/commit metadata bold,
/// `\ No newline` and `Binary files` dim, everything else (context, message
/// body) plain.
fn diff_line_style(line: &str) -> Style {
    const META: &[&str] = &[
        "diff --git",
        "index ",
        "--- ",
        "+++ ",
        "old mode",
        "new mode",
        "new file",
        "deleted file",
        "rename ",
        "copy ",
        "similarity ",
        "dissimilarity ",
        "commit ",
        "Author:",
        "AuthorDate:",
        "Commit:",
        "CommitDate:",
        "Date:",
        "Merge:",
    ];
    if line.starts_with("@@") {
        fg(HUNK)
    } else if line.starts_with("Binary files") || line.starts_with('\\') {
        Style::new().fg(IDLE).add_modifier(Modifier::DIM)
    } else if META.iter().any(|p| line.starts_with(p)) {
        Style::new().fg(IDLE).add_modifier(Modifier::BOLD)
    } else if line.starts_with('+') {
        fg(ADD)
    } else if line.starts_with('-') {
        fg(DEL)
    } else {
        fg(IDLE)
    }
}

/// A `git diff` / `git show` blob, coloured line by line. `focus`, when set, is
/// a 0-based line index that gets `REVERSED` so a `]` / `[` jump lands visibly.
pub fn diff_lines(raw: &str, focus: Option<usize>) -> Text<'static> {
    let lines = raw.lines().enumerate().map(|(i, line)| {
        let mut style = diff_line_style(line);
        if focus == Some(i) {
            style = style.add_modifier(Modifier::REVERSED);
        }
        Line::styled(line.to_string(), style)
    });
    Text::from(lines.collect::<Vec<_>>())
}

/// Render a parsed `Diff` for the right pane. Thin wrapper over `diff_lines`
/// over `diff.text`; the parse drives navigation, not colour.
pub fn render_diff(diff: &Diff, focus: Option<usize>) -> Text<'static> {
    diff_lines(&diff.text, focus)
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

/// Keybind bar, lazygit style: `Label: key | Label: key | ...`. The label is
/// dim, the key (everything after `: ` in a segment) is yellow. A trailing
/// segment without a colon (like `...`) stays dim.
pub fn keybar_line(raw: &'static str) -> Line<'static> {
    let mut spans = Vec::new();
    let segments: Vec<&str> = raw.split(" | ").collect();
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", fg(IDLE)));
        }
        match seg.split_once(": ") {
            Some((label, key)) => {
                spans.push(Span::styled(format!("{label}: "), fg(IDLE)));
                spans.push(Span::styled(key.to_string(), fg(KEY)));
            }
            None => spans.push(Span::styled(seg.to_string(), fg(IDLE))),
        }
    }
    Line::from(spans)
}
