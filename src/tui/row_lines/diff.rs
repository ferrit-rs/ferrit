//! Rendering for parsed diffs and ANSI pager output.

use crate::git::diff::Diff;
use crate::theme::palette::Palette;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use std::ops::Range;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SynColor, Theme as SynTheme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::tui::row_lines::{META, fg};

/// Bundled syntax definitions, loaded once. `_newlines` variant: its patterns
/// expect the trailing `\n` syntect's own examples use, which we don't have
/// per line here, but it also has the widest built-in language coverage.
fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// A bundled syntax theme: a dark one close to lazygit's own dark default, or
/// a light one when the palette is for a light terminal.
fn syntax_theme(light: bool) -> &'static SynTheme {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    static FALLBACK: OnceLock<SynTheme> = OnceLock::new();
    let name = if light {
        "InspiredGitHub"
    } else {
        "base16-ocean.dark"
    };
    THEMES
        .get_or_init(ThemeSet::load_defaults)
        .themes
        .get(name)
        .unwrap_or_else(|| FALLBACK.get_or_init(SynTheme::default))
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
fn diff_line_style(p: &Palette, line: &str) -> Style {
    if line.starts_with("@@") {
        fg(p.hunk)
    } else if line.starts_with("stash@{") {
        // The stash's own subject above its stat, yellow like lazygit's.
        fg(p.warn)
    } else if line.starts_with("Binary files") || line.starts_with('\\') {
        Style::new().fg(p.idle).add_modifier(Modifier::DIM)
    } else if META.iter().any(|p| line.starts_with(p)) {
        Style::new().fg(p.idle).add_modifier(Modifier::BOLD)
    } else if line.starts_with('+') {
        fg(p.add)
    } else if line == "---" {
        // The bare separator `git show --stat -p` prints before the stat block: not a
        // removal (a removed line of dashes would carry its own `-` and text).
        fg(p.idle)
    } else if line.starts_with('-') {
        fg(p.del)
    } else {
        fg(p.idle)
    }
}

/// A `git diff` / `git show` blob, coloured line by line. `focus`, when set, is
/// a 0-based line index that gets `REVERSED` so a `]` / `[` jump lands visibly.
pub fn diff_lines(p: &Palette, raw: &str, focus: Option<usize>) -> Text<'static> {
    let lines = raw.lines().enumerate().map(|(i, line)| {
        let mut style = diff_line_style(p, line);
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
/// `add`/`del` foreground plus a full-line `add_line_bg`/`del_line_bg`
/// pastel background, no per-token colour. Everything else falls back to
/// the flat `diff_line_style` colour. When `focus` is set, a `focus_box`
/// background boxes every line of the hunk (or file, in a commit) a `]` /
/// `[` jump last landed on, its header reversed.
pub fn render_diff(
    p: &Palette,
    diff: &Diff,
    focus: Option<&Range<usize>>,
    panel_width: usize,
) -> Text<'static> {
    let numbers = diff.line_numbers();
    let extensions = diff.line_extensions();
    let width = numbers
        .iter()
        .flat_map(|&pair| <[_; 2]>::from(pair))
        .flatten()
        .max()
        .map_or(3, |n| n.to_string().len());

    let set = syntax_set();
    let theme = syntax_theme(p.light);

    let mut out = Vec::with_capacity(diff.text.lines().count());
    for (i, line) in diff.text.lines().enumerate() {
        let boxed = focus.is_some_and(|r| r.contains(&i));
        let header = focus.is_some_and(|r| r.start == i);
        let overlay = |mut style: Style| -> Style {
            if boxed {
                style = style.bg(p.focus_box);
            }
            if header {
                style = style.add_modifier(Modifier::REVERSED);
            }
            style
        };

        let gutter_style = overlay(Style::new().fg(p.idle).add_modifier(Modifier::DIM));
        let (old, new) = numbers.get(i).copied().unwrap_or((None, None));
        let gutter = format!(
            "{:>w$} {:>w$}│",
            old.map_or_else(String::new, |n| n.to_string()),
            new.map_or_else(String::new, |n| n.to_string()),
            w = width
        );
        let mut spans = vec![Span::styled(gutter, gutter_style)];

        let changed = if line.starts_with('+') && !line.starts_with("+++") {
            Some(p.add_line_bg)
        } else if line.starts_with('-') && !line.starts_with("---") {
            Some(p.del_line_bg)
        } else {
            None
        };

        if let Some(bg) = changed {
            // `+`/`-` line: flat marker/body colour on the full-line
            // background, no syntax tokenizing.
            let base = Style::new().bg(bg);
            let text_fg = if line.starts_with('+') { p.add } else { p.del };
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
                        overlay(diff_line_style(p, line)),
                    ));
                },
            }
        }

        let mut rendered = Line::from(spans);
        if changed.is_some() || boxed {
            let fill_style = if changed.is_some() {
                let bg = if line.starts_with('+') {
                    p.add_line_bg
                } else {
                    p.del_line_bg
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
