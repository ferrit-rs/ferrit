//! Rendering for status and command-log lines.

use ferrit_domain::command_log::{CommandKind, CommandRecord};
use ferrit_domain::diff::DiffStat;
use ferrit_domain::model::StatusHeader;
use ferrit_theme::palette::Palette;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::fmt::Write as _;

use crate::ui::row_lines::fg;

/// `git --shortstat` style summary shown above a diff: `N file(s) changed, X
/// insertion(s)(+), Y deletion(s)(-)`, insertions in green, deletions in red.
/// A part is skipped when its count is zero, matching real `git` output.
pub fn stat_line(p: &Palette, stat: DiffStat) -> Line<'static> {
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
            fg(p.add),
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
            fg(p.del),
        ));
    }
    Line::from(spans)
}

/// The Status pane's first line: `ferrit -> main ↑2`, with a tick when the
/// branch is level with its upstream.
pub fn status_header(repo_name: &str, h: &StatusHeader) -> String {
    let mut line = format!("{repo_name} \u{2192} {}", h.branch);
    if h.ahead > 0 {
        let _ = write!(line, " \u{2191}{}", h.ahead);
    }
    if h.behind > 0 {
        let _ = write!(line, " \u{2193}{}", h.behind);
    }
    if h.upstream.is_some() && h.ahead == 0 && h.behind == 0 {
        line.push_str(" \u{2713}");
    }
    line
}

/// Repo status header line: highlight the ahead/behind arrows.
pub fn status_line(p: &Palette, raw: &str) -> Line<'static> {
    let style = if raw.contains('\u{2191}') || raw.contains('\u{2193}') {
        fg(p.warn)
    } else {
        fg(p.add)
    };
    Line::styled(raw.to_owned(), style)
}

/// A `refresh()` failure, surfaced in the Status pane instead of a panic.
pub fn error_line(p: &Palette, raw: &str) -> Line<'static> {
    Line::styled(raw.to_owned(), fg(p.del))
}

/// The "git is stopped mid-operation" badge (`REBASING 2/4`, `MERGING`):
/// bold in the warning colour, since the repository is waiting on the user.
pub fn operation_line(p: &Palette, label: &str) -> Line<'static> {
    Line::styled(label.to_owned(), fg(p.warn).add_modifier(Modifier::BOLD))
}

/// A background fetch/pull/push in flight, shown under the main status
/// line while `App::remote_busy_label` is `Some`. Dim, matching `Note`
/// diff view's dim style — not an error, not a success, just "wait".
/// See `docs/PLAN_9_REMOTE.md`.
pub fn busy_line(p: &Palette, label: &str) -> Line<'static> {
    Line::styled(
        label.to_owned(),
        Style::new().fg(p.idle).add_modifier(Modifier::DIM),
    )
}

/// Command-log line: dim the `$` prompt, leave the command bright.
pub fn log_line(p: &Palette, raw: &'static str) -> Line<'static> {
    match raw.strip_prefix("$ ") {
        Some(cmd) => Line::from(vec![Span::styled("$ ", fg(p.idle)), Span::raw(cmd)]),
        None => Line::styled(raw, fg(p.idle)),
    }
}

/// The lines a record takes in the Infos box: the command, then every line of git's own
/// answer to it (`[branch hash] summary`, the stat, `create mode` after a commit), as lazygit's command log
/// does under "Git output".
pub fn command_lines(p: &Palette, record: &CommandRecord) -> Vec<Line<'static>> {
    let mut lines = vec![command_line(p, record)];
    lines.extend(
        record
            .output
            .iter()
            .map(|line| Line::styled(line.clone(), fg(p.idle))),
    );
    lines
}

/// Command-log line for a recorded subprocess: dim `$`, the command, and a
/// red note when it failed. Reads (only listed in the full viewer) are dim.
pub fn command_line(p: &Palette, record: &CommandRecord) -> Line<'static> {
    let command_style = if record.failed() {
        fg(p.del)
    } else if record.kind == CommandKind::Read {
        fg(p.idle)
    } else {
        Style::new()
    };
    let mut spans = vec![
        Span::styled("$ ", fg(p.idle)),
        Span::styled(record.argv.clone(), command_style),
    ];
    if record.failed() {
        let note = match record.exit {
            Some(code) => format!("  (exit {code})"),
            None => "  (not completed)".to_owned(),
        };
        spans.push(Span::styled(note, fg(p.del)));
    }
    Line::from(spans)
}
