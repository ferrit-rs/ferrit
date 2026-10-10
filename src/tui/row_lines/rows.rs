//! Rendering for list and history rows.

use crate::git::model::{
    BranchEntry, Change, CommitEntry, CommitRefKind, FileEntry, PushState, RemoteEntry, StashEntry,
};
use crate::theme::palette::Palette;
use crate::tui::components::panes::tree::StageState;
use crate::tui::row_lines::fg;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Style for the selected row in a left-pane list. Like lazygit: a solid blue
/// bar only in the focused pane, filled across the pane width by the `List`
/// widget. Unfocused panes keep a cursor position but draw no bar, so only one
/// selection reads as "live" at a time.
pub fn selection_style(p: &Palette, focused: bool) -> Style {
    if focused {
        Style::new()
            .bg(p.selection)
            .fg(p.selection_fg)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    }
}

/// Bottom-right `N of M` counter shown on each list pane's border.
pub fn counter_line(p: &Palette, current: usize, total: usize) -> Line<'static> {
    Line::styled(format!(" {current} of {total} "), fg(p.idle)).right_aligned()
}

/// The commit subject length the counter measures against (the "50/72" rule).
pub const SUBJECT_LIMIT: usize = 50;

/// Bottom-right `n/50` counter of the commit summary: dim while the subject
/// fits, the warning colour once it is longer. A hint, never a block.
pub fn subject_counter(p: &Palette, length: usize) -> Line<'static> {
    let colour = if length > SUBJECT_LIMIT {
        p.warn
    } else {
        p.idle
    };
    Line::styled(format!(" {length}/{SUBJECT_LIMIT} "), fg(colour)).right_aligned()
}

/// Two spaces per tree depth, lazygit's own indent width.
fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

/// Files row, porcelain layout `XY path` (X = staged, Y = worktree), coloured as
/// lazygit does: the staged letter green, the unstaged one red, so a staged `M` and
/// an unstaged `M` no longer look alike, and an untracked file is `??` in red. `depth` indents
/// under the row's parent directory in the tree view (`App::file_lines`);
/// when nested (`depth > 0`), only the file's own name shows, not the full
/// path — the parent directory rows above it already say where it lives.
pub fn file_line(p: &Palette, entry: &FileEntry, depth: usize) -> Line<'static> {
    let untracked = entry.staged == Change::Untracked || entry.worktree == Change::Untracked;
    let conflicted = entry.staged == Change::Conflicted || entry.worktree == Change::Conflicted;
    let (staged, unstaged) = if untracked {
        ("?".to_owned(), "?".to_owned())
    } else {
        (
            entry.staged.code().to_string(),
            entry.worktree.code().to_string(),
        )
    };
    let staged_color = if untracked || conflicted {
        p.del
    } else {
        p.add
    };
    let name = if depth == 0 {
        entry.path.display().to_string()
    } else {
        entry.path.file_name().map_or_else(
            || entry.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    };
    // lazygit greens the name of a fully staged file, letters included, and
    // yellows one that is staged and changed again (`MM`).
    let name_style = if entry.is_fully_staged() {
        fg(p.add)
    } else if entry.has_staged() {
        fg(p.warn)
    } else {
        Style::new()
    };
    Line::from(vec![
        Span::raw(indent(depth)),
        Span::styled(staged, fg(staged_color)),
        Span::styled(unstaged, fg(p.del)),
        Span::styled(format!(" {name}"), name_style),
    ])
}

/// Directory row in the Files tree: an expand/collapse arrow (`▼`/`▶`, like
/// lazygit) then the directory's own name, indented to its depth. No status
/// code; the arrow and name are green when everything under the directory is
/// staged and yellow when only part of it is, as in lazygit.
pub(crate) fn dir_line(
    p: &Palette,
    name: &str,
    depth: usize,
    expanded: bool,
    stage: StageState,
) -> Line<'static> {
    let arrow = if expanded { "\u{25bc} " } else { "\u{25b6} " };
    let (arrow_style, name_style) = match stage {
        StageState::All => (fg(p.add), fg(p.add)),
        StageState::Partial => (fg(p.warn), fg(p.warn)),
        StageState::None => (fg(p.idle), Style::new()),
    };
    Line::from(vec![
        Span::raw(indent(depth)),
        Span::styled(arrow, arrow_style),
        Span::styled(name.to_owned(), name_style.add_modifier(Modifier::BOLD)),
    ])
}

/// The selected row keeps the colours its spans carry (lazygit only adds the
/// bar and bold); spans with no colour of their own take `selection_fg`.
/// Pair with `selection_style` minus its `fg`.
pub fn keep_colours_on_selection(p: &Palette, line: &mut Line<'static>) {
    for span in &mut line.spans {
        span.style.fg.get_or_insert(p.selection_fg);
    }
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
/// there is an upstream to compare against, and, during remote work, the
/// LazyGit-style inline operation status.
pub fn branch_line_with_status(
    p: &Palette,
    entry: &BranchEntry,
    operation: Option<&str>,
) -> Line<'static> {
    let marker = if entry.is_head { "* " } else { "  " };
    let name_style = if entry.is_head {
        Style::new().fg(p.add).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    let mut spans = vec![
        Span::styled(format!("{:<3}", relative_age(entry.tip_time)), fg(p.hunk)),
        Span::styled(marker, fg(p.add)),
        Span::styled(entry.name.clone(), name_style),
    ];
    if let Some(operation) = operation {
        spans.push(Span::styled(format!(" {operation}"), fg(p.hunk)));
    } else if entry.upstream.is_some() {
        if entry.ahead == 0 && entry.behind == 0 {
            spans.push(Span::styled(" \u{2713}", fg(p.add)));
        }
        if entry.ahead > 0 {
            spans.push(Span::styled(
                format!(" \u{2191}{}", entry.ahead),
                fg(p.warn),
            ));
        }
        if entry.behind > 0 {
            spans.push(Span::styled(
                format!(" \u{2193}{}", entry.behind),
                fg(p.warn),
            ));
        }
    }
    Line::from(spans)
}

/// Branches pane's Remotes tab row: `name  fetch: <url>  push: <url>`, the
/// push URL omitted when it is identical to fetch (the common case).
/// `docs/PLAN_9_REMOTE.md`; no selection styling, this tab has no cursor.
pub fn remote_line(p: &Palette, entry: &RemoteEntry) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            entry.name.clone(),
            Style::new().fg(p.hash).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  fetch: ", fg(p.idle)),
        Span::raw(entry.fetch_url.clone()),
    ];
    if entry.push_url != entry.fetch_url {
        spans.push(Span::styled("  push: ", fg(p.idle)));
        spans.push(Span::raw(entry.push_url.clone()));
    }
    Line::from(spans)
}

/// lazygit commit row: `<hash> <initials> <graph-node> <subject>`. Hash and
/// node in green, author initials in magenta, subject plain. `graph` is the
/// graph-column glyph, a plain `o` for linear history until phase 2 G4.
pub fn commit_line(p: &Palette, entry: &CommitEntry) -> Line<'static> {
    // lazygit's hash colours: red not pushed yet, yellow pushed, green merged into
    // the remote's main branch.
    let hash = match entry.push_state {
        PushState::Unpushed => p.del,
        PushState::Pushed => p.warn,
        PushState::Merged => p.hash,
    };
    let mut spans = vec![
        Span::styled(entry.short_hash.clone(), fg(hash)),
        Span::raw(" "),
        Span::styled(entry.author_initials(), fg(p.author)),
        Span::raw(" "),
        Span::styled("o", fg(p.hash)),
        Span::raw(" "),
    ];
    // Tags sit before the subject, in bold magenta.
    for tag in entry.tags() {
        spans.push(Span::styled(
            tag.to_owned(),
            fg(p.author).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::raw(entry.summary.clone()));
    Line::from(spans)
}

/// `(HEAD -> main, tag: v0.6.0, origin/main)` as spans, each name in its own colour: the
/// checked-out branch and `HEAD` green, other branches green, tags yellow, remotes red.
/// Empty when nothing points at the commit.
fn decoration_spans(p: &Palette, entry: &CommitEntry) -> Vec<Span<'static>> {
    if entry.refs.is_empty() {
        return Vec::new();
    }
    let mut spans = vec![Span::raw(" (")];
    for (i, reference) in entry.refs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(", "));
        }
        let style = match reference.kind {
            CommitRefKind::Head => fg(p.add).add_modifier(Modifier::BOLD),
            CommitRefKind::Branch => fg(p.add),
            CommitRefKind::Tag => fg(p.warn).add_modifier(Modifier::BOLD),
            CommitRefKind::Remote => fg(p.del),
        };
        spans.push(Span::styled(reference.label.clone(), style));
    }
    spans.push(Span::raw(")"));
    spans
}

/// Line count of one `branch_log_block` entry. Kept in sync with it so the
/// scroll clamp knows the real height without re-building the styled lines.
pub fn branch_log_block_lines(entry: &CommitEntry) -> usize {
    let body = entry.body.lines().count();
    6 + if body > 0 { body + 1 } else { 0 }
}

/// One commit as a multi-line, git-log-style block: hash, author, relative
/// date, and the summary indented under a `|` continuation — closer to
/// lazygit's Log panel than the compact `commit_line` row `Commits` uses.
/// There is room for it since this is the passive Branches-pane preview
/// (`DiffView::BranchLog`), which fills the whole right pane rather than a
/// narrow list column.
pub fn branch_log_block(p: &Palette, entry: &CommitEntry) -> Vec<Line<'static>> {
    let graph = fg(p.hunk);
    let label = fg(p.idle);
    let mut header = vec![
        Span::styled("* ", graph),
        Span::styled("commit ", label),
        Span::styled(entry.short_hash.clone(), fg(p.hash)),
    ];
    header.extend(decoration_spans(p, entry));
    let mut lines = vec![
        Line::from(header),
        Line::from(vec![
            Span::styled("| ", graph),
            Span::styled("Author: ", label),
            Span::raw(format!("{} <{}>", entry.author, entry.author_email)),
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
    ];
    // The message body, as lazygit prints it, under the subject and a blank row.
    if !entry.body.is_empty() {
        for text in entry.body.lines() {
            lines.push(Line::from(vec![
                Span::styled("| ", graph),
                Span::raw(format!("    {text}")),
            ]));
        }
        lines.push(Line::from(Span::styled("|", graph)));
    }
    lines
}

/// lazygit stash row: `stash@{0}: message`.
pub fn stash_line(p: &Palette, entry: &StashEntry) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("stash@{{{}}}", entry.index), fg(p.hash)),
        Span::raw(": "),
        Span::raw(entry.message.clone()),
    ])
}
