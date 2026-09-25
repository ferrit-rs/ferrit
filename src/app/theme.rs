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

use crate::domain::git::command_log::{CommandKind, CommandRecord};
use crate::domain::git::diff::{Diff, DiffStat};
use crate::domain::git::model::FileEntry;
use crate::domain::git::model::RemoteEntry;
use crate::domain::git::model::{BranchEntry, CommitEntry, StashEntry};

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

/// Two spaces per tree depth, lazygit's own indent width.
fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

/// Files row, porcelain layout `XY path` (X = staged, Y = worktree): colour the
/// two-char code by what it means, leave the path plain. `depth` indents
/// under the row's parent directory in the tree view (`App::file_lines`);
/// when nested (`depth > 0`), only the file's own name shows, not the full
/// path — the parent directory rows above it already say where it lives.
pub fn file_line(entry: &FileEntry, depth: usize) -> Line<'static> {
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
    let name = if depth == 0 {
        entry.path.display().to_string()
    } else {
        entry.path.file_name().map_or_else(
            || entry.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    };
    Line::from(vec![
        Span::raw(indent(depth)),
        Span::styled(code, fg(color)),
        Span::raw(format!(" {name}")),
    ])
}

/// Directory row in the Files tree: an expand/collapse arrow (`▼`/`▶`, like
/// lazygit) then the directory's own name, indented to its depth. No status
/// code — files carry their own, a directory's would need aggregating
/// several and lazygit doesn't bother either.
pub fn dir_line(name: &str, depth: usize, expanded: bool) -> Line<'static> {
    let arrow = if expanded { "\u{25bc} " } else { "\u{25b6} " };
    Line::from(vec![
        Span::raw(indent(depth)),
        Span::styled(arrow, fg(IDLE)),
        Span::styled(name.to_owned(), Style::new().add_modifier(Modifier::BOLD)),
    ])
}

/// Relative age, lazygit's branch-list recency column and Log panel `Date:`
/// line: the largest whole unit — `3d`, `5h`, `12m`, `9s` — rather than
/// always flooring to days, which prints a misleading `0d` for anything
/// committed earlier today. No date crate: plain seconds-since-`tip_time`
/// division, clamped to 0 for a clock skew or a commit newer than "now".
fn relative_age(tip_time: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(tip_time, |d| i64::try_from(d.as_secs()).unwrap_or(tip_time));
    let secs = (now - tip_time).max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3_600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3_600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

/// lazygit branch row: `1d * main ↑2` for the checked-out branch (green,
/// bold), `3d   feat/x` for the rest. Ahead/behind arrows in yellow when
/// there is an upstream to compare against.
pub fn branch_line(entry: &BranchEntry) -> Line<'static> {
    branch_line_with_status(entry, None)
}

/// Branch row with LazyGit-style inline operation status during remote work.
pub fn branch_line_with_status(entry: &BranchEntry, operation: Option<&str>) -> Line<'static> {
    let marker = if entry.is_head { "* " } else { "  " };
    let name_style = if entry.is_head {
        Style::new().fg(ADD).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    let mut spans = vec![
        Span::styled(format!("{:<3}", relative_age(entry.tip_time)), fg(HUNK)),
        Span::styled(marker, fg(ADD)),
        Span::styled(entry.name.clone(), name_style),
    ];
    if let Some(operation) = operation {
        spans.push(Span::styled(format!(" {operation}"), fg(HUNK)));
    } else if entry.upstream.is_some() {
        if entry.ahead > 0 {
            spans.push(Span::styled(format!(" \u{2191}{}", entry.ahead), fg(WARN)));
        }
        if entry.behind > 0 {
            spans.push(Span::styled(format!(" \u{2193}{}", entry.behind), fg(WARN)));
        }
    }
    Line::from(spans)
}

/// Branches pane's Remotes tab row: `name  fetch: <url>  push: <url>`, the
/// push URL omitted when it is identical to fetch (the common case).
/// `docs/PLAN_9_REMOTE.md`; no selection styling, this tab has no cursor.
pub fn remote_line(entry: &RemoteEntry) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            entry.name.clone(),
            Style::new().fg(HASH).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  fetch: ", fg(IDLE)),
        Span::raw(entry.fetch_url.clone()),
    ];
    if entry.push_url != entry.fetch_url {
        spans.push(Span::styled("  push: ", fg(IDLE)));
        spans.push(Span::raw(entry.push_url.clone()));
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

/// Line count of one `branch_log_block` entry. Kept in sync with it so the
/// scroll clamp knows the real height without re-building the styled lines.
pub const BRANCH_LOG_BLOCK_LINES: usize = 6;

/// One commit as a multi-line, git-log-style block: hash, author, relative
/// date, and the summary indented under a `|` continuation — closer to
/// lazygit's Log panel than the compact `commit_line` row `Commits` uses.
/// There is room for it since this is the passive Branches-pane preview
/// (`DiffView::BranchLog`), which fills the whole right pane rather than a
/// narrow list column.
pub fn branch_log_block(entry: &CommitEntry) -> Vec<Line<'static>> {
    let graph = fg(HUNK);
    let label = fg(IDLE);
    vec![
        Line::from(vec![
            Span::styled("* ", graph),
            Span::styled("commit ", label),
            Span::styled(entry.short_hash.clone(), fg(HASH)),
        ]),
        Line::from(vec![
            Span::styled("| ", graph),
            Span::styled("Author: ", label),
            Span::raw(entry.author.clone()),
        ]),
        Line::from(vec![
            Span::styled("| ", graph),
            Span::styled("Date:   ", label),
            Span::raw(format!("{} ago", relative_age(entry.time))),
        ]),
        Line::from(Span::styled("|", graph)),
        Line::from(vec![
            Span::styled("| ", graph),
            Span::raw(format!("    {}", entry.summary)),
        ]),
        Line::from(Span::styled("|", graph)),
    ]
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

/// Convert delta's ANSI pager output into ratatui lines. Delta owns structural
/// formatting and word-level highlighting; this small SGR reader preserves its
/// foreground/background styles without sending escape sequences to terminal.
pub fn render_delta(raw: &str, panel_width: usize) -> Text<'static> {
    let mut lines = Vec::new();
    for raw_line in raw.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        lines.push(parse_ansi_line(
            line.strip_suffix('\r').unwrap_or(line),
            panel_width,
        ));
    }
    Text::from(lines)
}

fn parse_ansi_line(raw: &str, panel_width: usize) -> Line<'static> {
    let mut spans = Vec::new();
    let mut style = Style::new();
    let mut background = None;
    let mut text_start = 0;
    let mut i = 0;
    while i < raw.len() {
        if raw.as_bytes().get(i) != Some(&0x1b) {
            i += 1;
            continue;
        }
        if text_start < i {
            spans.push(Span::styled(raw[text_start..i].to_owned(), style));
        }
        let Some(rest) = raw.get(i + 1..) else { break };
        if let Some(sequence) = rest.strip_prefix('[') {
            let Some(end) = sequence.find(|c: char| c.is_ascii_alphabetic()) else {
                break;
            };
            let Some(&final_byte) = sequence.as_bytes().get(end) else {
                break;
            };
            if final_byte == b'm' {
                let params = &sequence[..end];
                apply_sgr(params, &mut style, &mut background);
            }
            i += 2 + end + 1;
        } else {
            i += 2;
        }
        text_start = i;
    }
    if text_start < raw.len() {
        spans.push(Span::styled(raw[text_start..].to_owned(), style));
    }
    let mut line = Line::from(spans);
    if background.is_some() {
        let padding = panel_width.saturating_sub(line.width());
        if padding > 0 {
            line.spans.push(Span::styled(" ".repeat(padding), style));
        }
    }
    line
}

fn apply_sgr(params: &str, style: &mut Style, background: &mut Option<Color>) {
    let values: Vec<u16> = if params.is_empty() {
        vec![0]
    } else {
        params
            .split(';')
            .filter_map(|p| p.parse::<u16>().ok())
            .collect()
    };
    let mut i = 0;
    while let Some(&code) = values.get(i) {
        match code {
            0 => {
                *style = Style::new();
                *background = None;
            },
            1 => *style = style.add_modifier(Modifier::BOLD),
            22 => *style = style.remove_modifier(Modifier::BOLD),
            7 => *style = style.add_modifier(Modifier::REVERSED),
            27 => *style = style.remove_modifier(Modifier::REVERSED),
            30..=37 => *style = style.fg(ansi_basic_color(code - 30, false)),
            90..=97 => *style = style.fg(ansi_basic_color(code - 90, true)),
            40..=47 => {
                let color = ansi_basic_color(code - 40, false);
                *background = Some(color);
                *style = style.bg(color);
            },
            100..=107 => {
                let color = ansi_basic_color(code - 100, true);
                *background = Some(color);
                *style = style.bg(color);
            },
            38 | 48 => {
                let is_background = code == 48;
                let Some(&mode) = values.get(i + 1) else {
                    break;
                };
                let Some((color, consumed)) = (match mode {
                    5 => values.get(i + 2).map(|&n| (Color::Indexed(to_u8(n)), 3)),
                    2 => match (values.get(i + 2), values.get(i + 3), values.get(i + 4)) {
                        (Some(&r), Some(&g), Some(&b)) => {
                            Some((Color::Rgb(to_u8(r), to_u8(g), to_u8(b)), 5))
                        },
                        _ => None,
                    },
                    _ => None,
                }) else {
                    break;
                };
                if is_background {
                    *background = Some(color);
                    *style = style.bg(color);
                } else {
                    *style = style.fg(color);
                }
                i += consumed - 1;
            },
            _ => {},
        }
        i += 1;
    }
}

fn to_u8(value: u16) -> u8 {
    u8::try_from(value.min(u16::from(u8::MAX))).unwrap_or(u8::MAX)
}

fn ansi_basic_color(index: u16, bright: bool) -> Color {
    let colors = if bright {
        [
            Color::Gray,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::White,
        ]
    } else {
        [
            Color::Black,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Gray,
        ]
    };
    colors
        .get(usize::from(index.min(7)))
        .copied()
        .unwrap_or(Color::White)
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

/// The "git is stopped mid-operation" badge (`REBASING 2/4`, `MERGING`):
/// bold in the warning colour, since the repository is waiting on the user.
pub fn operation_line(label: &str) -> Line<'static> {
    Line::styled(label.to_owned(), fg(WARN).add_modifier(Modifier::BOLD))
}

/// A background fetch/pull/push in flight, shown under the main status
/// line while `App::remote_busy_label` is `Some`. Dim, matching `Note`
/// diff view's dim style — not an error, not a success, just "wait".
/// See `docs/PLAN_9_REMOTE.md`.
pub fn busy_line(label: &str) -> Line<'static> {
    Line::styled(
        label.to_owned(),
        Style::new().fg(IDLE).add_modifier(Modifier::DIM),
    )
}

/// Command-log line: dim the `$` prompt, leave the command bright.
pub fn log_line(raw: &'static str) -> Line<'static> {
    match raw.strip_prefix("$ ") {
        Some(cmd) => Line::from(vec![Span::styled("$ ", fg(IDLE)), Span::raw(cmd)]),
        None => Line::styled(raw, fg(IDLE)),
    }
}

/// Command-log line for a recorded subprocess: dim `$`, the command, and a
/// red note when it failed. Reads (only listed in the full viewer) are dim.
pub fn command_line(record: &CommandRecord) -> Line<'static> {
    let command_style = if record.failed() {
        fg(DEL)
    } else if record.kind == CommandKind::Read {
        fg(IDLE)
    } else {
        Style::new()
    };
    let mut spans = vec![
        Span::styled("$ ", fg(IDLE)),
        Span::styled(record.argv.clone(), command_style),
    ];
    if record.failed() {
        let note = match record.exit {
            Some(code) => format!("  (exit {code})"),
            None => "  (not completed)".to_owned(),
        };
        spans.push(Span::styled(note, fg(DEL)));
    }
    Line::from(spans)
}
