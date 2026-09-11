//! Colour palette and the span builders that give each kind of line its
//! meaning-carrying colour. One flat palette tuned to match lazygit's default
//! theme: green for the focused pane, a solid blue selection bar, green hashes,
//! yellow keys. No config, no theme switching yet (that is phase 10).

use std::ops::Range;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SynColor, Theme as SynTheme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::git::{BranchEntry, CommitEntry, Diff, DiffStat, FileEntry, StashEntry};

/// Prefixes of diff metadata lines (file/commit headers), never source code.
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
/// Background tint boxing the hunk (or file, in a commit) a `]` / `[` jump
/// last landed on.
pub const FOCUS_BOX: Color = Color::DarkGray;
/// Full-line pastel background tint on a `+` line, under its syntax-coloured
/// text (`render_diff`).
pub const ADD_LINE_BG: Color = Color::Rgb(20, 45, 20);
/// Full-line pastel background tint on a `-` line, under its syntax-coloured
/// text.
pub const DEL_LINE_BG: Color = Color::Rgb(55, 20, 20);

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

/// Bundled syntax definitions, loaded once. `_newlines` variant: its patterns
/// expect the trailing `\n` syntect's own examples use, which we don't have
/// per line here, but it also has the widest built-in language coverage.
fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// A single bundled dark theme, close to lazygit's own dark default.
fn syntax_theme() -> &'static SynTheme {
    static THEME: OnceLock<SynTheme> = OnceLock::new();
    THEME.get_or_init(|| {
        ThemeSet::load_defaults()
            .themes
            .remove("base16-ocean.dark")
            .unwrap_or_default()
    })
}

fn to_color(c: SynColor) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Is `line` source code a syntax highlighter should tokenize, rather than
/// diff metadata (headers, hunk markers, binary/no-newline notices)?
fn is_code_line(line: &str) -> bool {
    !line.starts_with("@@")
        && !line.starts_with("Binary files")
        && !line.starts_with('\\')
        && !META.iter().any(|p| line.starts_with(p))
}

/// Colour of one diff line, by its leading bytes. Matches what `git --color`
/// paints: hunk header cyan, `+`/`-` green/red, file/commit metadata bold,
/// `\ No newline` and `Binary files` dim, everything else (context, message
/// body) plain.
fn diff_line_style(line: &str) -> Style {
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
        Line::styled(line.to_owned(), style)
    });
    Text::from(lines.collect::<Vec<_>>())
}

/// Render a parsed `Diff` for the right pane: a lazygit-style `old new│`
/// gutter from `Diff::line_numbers` in front of every line (blank on
/// headers, one-sided on an addition/deletion). Syntax colour and the
/// full-line add/delete background are mutually exclusive, lazygit/lazygitrs
/// pager style: a context line (not a file/commit header, hunk marker, or
/// binary/no-newline notice) is tokenized by `syntect` for its per-language
/// foreground colour on a plain background; a `+`/`-` line instead gets flat
/// `ADD`/`DEL` foreground plus a full-line `ADD_LINE_BG`/`DEL_LINE_BG`
/// pastel background, no per-token colour. Everything else falls back to
/// the flat `diff_line_style` colour. When `focus` is set, a `FOCUS_BOX`
/// background boxes every line of the hunk (or file, in a commit) a `]` /
/// `[` jump last landed on, its header reversed.
pub fn render_diff(diff: &Diff, focus: Option<&Range<usize>>, panel_width: usize) -> Text<'static> {
    let numbers = diff.line_numbers();
    let extensions = diff.line_extensions();
    let width = numbers
        .iter()
        .flat_map(|&pair| <[_; 2]>::from(pair))
        .flatten()
        .max()
        .map_or(3, |n| n.to_string().len());

    let set = syntax_set();
    let theme = syntax_theme();

    let mut out = Vec::with_capacity(diff.text.lines().count());
    for (i, line) in diff.text.lines().enumerate() {
        let boxed = focus.is_some_and(|r| r.contains(&i));
        let header = focus.is_some_and(|r| r.start == i);
        let overlay = |mut style: Style| -> Style {
            if boxed {
                style = style.bg(FOCUS_BOX);
            }
            if header {
                style = style.add_modifier(Modifier::REVERSED);
            }
            style
        };

        let gutter_style = overlay(Style::new().fg(IDLE).add_modifier(Modifier::DIM));
        let (old, new) = numbers.get(i).copied().unwrap_or((None, None));
        let gutter = format!(
            "{:>w$} {:>w$}│",
            old.map_or_else(String::new, |n| n.to_string()),
            new.map_or_else(String::new, |n| n.to_string()),
            w = width
        );
        let mut spans = vec![Span::styled(gutter, gutter_style)];

        let changed = if line.starts_with('+') && !line.starts_with("+++") {
            Some(ADD_LINE_BG)
        } else if line.starts_with('-') && !line.starts_with("---") {
            Some(DEL_LINE_BG)
        } else {
            None
        };

        if let Some(bg) = changed {
            // `+`/`-` line: flat marker/body colour on the full-line
            // background, no syntax tokenizing.
            let base = Style::new().bg(bg);
            let text_fg = if line.starts_with('+') { ADD } else { DEL };
            spans.push(Span::styled(line.to_owned(), overlay(base.fg(text_fg))));
        } else {
            // Context (or header/hunk-marker/binary line): syntax colour
            // when eligible, flat `diff_line_style` otherwise.
            let ext = extensions.get(i).cloned().flatten();
            let syntax = is_code_line(line)
                .then_some(ext.as_deref())
                .flatten()
                .and_then(|ext| set.find_syntax_by_extension(ext));
            match syntax {
                Some(syntax) => {
                    let marker_len = usize::from(line.starts_with(' '));
                    let marker = line.get(..marker_len).unwrap_or_default();
                    let body = line.get(marker_len..).unwrap_or_default();
                    if !marker.is_empty() {
                        spans.push(Span::styled(marker.to_owned(), overlay(Style::new())));
                    }
                    let mut hl = HighlightLines::new(syntax, theme);
                    let tokens = hl
                        .highlight_line(body, set)
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>();
                    if tokens.is_empty() {
                        spans.push(Span::styled(body.to_owned(), overlay(Style::new())));
                    }
                    for (style, text) in tokens {
                        let span_style = fg(to_color(style.foreground));
                        spans.push(Span::styled(text.to_owned(), overlay(span_style)));
                    }
                },
                None => {
                    spans.push(Span::styled(
                        line.to_owned(),
                        overlay(diff_line_style(line)),
                    ));
                },
            }
        }

        let mut rendered = Line::from(spans);
        if changed.is_some() || boxed {
            let fill_style = if changed.is_some() {
                let bg = if line.starts_with('+') {
                    ADD_LINE_BG
                } else {
                    DEL_LINE_BG
                };
                overlay(Style::new().bg(bg))
            } else {
                overlay(Style::new())
            };
            let padding = panel_width.saturating_sub(rendered.width());
            if padding > 0 {
                rendered
                    .spans
                    .push(Span::styled(" ".repeat(padding), fill_style));
            }
        }
        out.push(rendered);
    }
    Text::from(out)
}

/// `git --shortstat` style summary shown above a diff: `N file(s) changed, X
/// insertion(s)(+), Y deletion(s)(-)`, insertions in green, deletions in red.
/// A part is skipped when its count is zero, matching real `git` output.
pub fn stat_line(stat: DiffStat) -> Line<'static> {
    let mut spans = vec![Span::raw(format!(
        "{} file{} changed",
        stat.files,
        if stat.files == 1 { "" } else { "s" }
    ))];
    if stat.insertions > 0 {
        spans.push(Span::raw(", "));
        spans.push(Span::styled(
            format!(
                "{} insertion{}(+)",
                stat.insertions,
                if stat.insertions == 1 { "" } else { "s" }
            ),
            fg(ADD),
        ));
    }
    if stat.deletions > 0 {
        spans.push(Span::raw(", "));
        spans.push(Span::styled(
            format!(
                "{} deletion{}(-)",
                stat.deletions,
                if stat.deletions == 1 { "" } else { "s" }
            ),
            fg(DEL),
        ));
    }
    Line::from(spans)
}

/// Repo status header line: highlight the ahead/behind arrows.
pub fn status_line(raw: &str) -> Line<'static> {
    let style = if raw.contains('\u{2191}') || raw.contains('\u{2193}') {
        fg(WARN)
    } else {
        fg(ADD)
    };
    Line::styled(raw.to_owned(), style)
}

/// A `refresh()` failure, surfaced in the Status pane instead of a panic.
pub fn error_line(raw: &str) -> Line<'static> {
    Line::styled(raw.to_owned(), fg(DEL))
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
                spans.push(Span::styled(key.to_owned(), fg(KEY)));
            },
            None => spans.push(Span::styled(seg.to_string(), fg(IDLE))),
        }
    }
    Line::from(spans)
}
