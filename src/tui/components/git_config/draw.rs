//! The git config screen: `draw`.

use crate::git::config::{Scope, WriteScope, display_value};
use crate::theme::palette::Palette;
use crate::tui::components::git_config::screen::ConfigRow;
use crate::tui::widgets::chrome::lists::ScrollBar;
use crate::tui::widgets::chrome::panel::Panel;
use crate::tui::widgets::chrome::text::cut_end;
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

/// Full markers from this width.
pub(crate) const WIDE: u16 = 100;
/// Compact markers from this width; under it none.
pub(crate) const NARROW: u16 = 60;
/// Cells the full marker may take.
pub(crate) const MARKER_WIDE: usize = 26;
/// Cells the compact marker may take.
pub(crate) const MARKER_COMPACT: usize = 6;
/// Longest key column.
pub(crate) const KEY_MAX: usize = 40;
/// Shortest key column worth showing.
pub(crate) const KEY_MIN: usize = 12;

/// What the screen draws, all of it given.
#[derive(Debug)]
pub(crate) struct View<'a> {
    pub rows: &'a [ConfigRow],
    pub selected: usize,
    /// The first item (a row or a section rule) of the last frame.
    pub offset: usize,
    pub scope: WriteScope,
    /// Values git knows, filter or not.
    pub total: usize,
    pub filter: &'a str,
    /// Typing goes to the filter: it shows a caret.
    pub filtering: bool,
    /// What the last action did.
    pub note: Option<&'a str>,
    pub palette: Palette,
}

/// A line of the list: a section rule or the row at this index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Item<'a> {
    Rule(&'a str),
    Row(usize),
}

/// The part of a key before its first dot: `core` for `core.editor`.
pub(crate) fn section(key: &str) -> &str {
    key.split_once('.').map_or(key, |(section, _)| section)
}

pub(crate) fn items(rows: &[ConfigRow]) -> Vec<Item<'_>> {
    let mut out = Vec::with_capacity(rows.len() * 2);
    let mut last = None;
    for (i, row) in rows.iter().enumerate() {
        let name = section(&row.entry.key);
        if last != Some(name) {
            out.push(Item::Rule(name));
            last = Some(name);
        }
        out.push(Item::Row(i));
    }
    out
}

pub(crate) const fn scope_letter(scope: Scope) -> char {
    match scope {
        Scope::System => 'S',
        Scope::Global => 'G',
        Scope::Local => 'L',
        Scope::Worktree => 'W',
        Scope::Command => 'C',
    }
}

pub(crate) fn scope_word(scope: Scope) -> &'static str {
    scope.into()
}

/// What the marker column says about a row.
pub(crate) fn marker(row: &ConfigRow, full: bool) -> String {
    let mut parts = Vec::new();
    if row.winner {
        parts.push(if full {
            format!("← wins ({})", scope_word(row.entry.scope))
        } else {
            "←".to_owned()
        });
    }
    if row.included {
        parts.push(if full { "inherited" } else { "inh" }.to_owned());
    }
    match row.entry.scope {
        Scope::System | Scope::Command => {
            parts.push(if full {
                format!("{}, r/o", scope_word(row.entry.scope))
            } else {
                "r/o".to_owned()
            });
        },
        Scope::Global | Scope::Local | Scope::Worktree => {},
    }
    parts.join(", ")
}

/// The row's own colours: a local override green, another winner yellow, a
/// shadowed value dim.
pub(crate) fn row_style(row: &ConfigRow, palette: &Palette) -> Style {
    if row.shadowed {
        Style::new().add_modifier(Modifier::DIM)
    } else if row.winner && row.entry.scope == Scope::Local {
        Style::new().fg(palette.add)
    } else if row.winner {
        Style::new().fg(palette.warn)
    } else {
        Style::new()
    }
}

pub(crate) fn pad(text: &str, width: usize) -> String {
    let cut = cut_end(text, width);
    let used = UnicodeWidthStr::width(cut.as_str());
    format!("{cut}{}", " ".repeat(width.saturating_sub(used)))
}

/// `── core ─────…` to `width` cells.
pub(crate) fn rule(name: &str, width: usize) -> String {
    let head = format!("── {name} ");
    let used = UnicodeWidthStr::width(head.as_str());
    format!("{head}{}", "─".repeat(width.saturating_sub(used)))
}

pub(crate) fn row_line(
    row: &ConfigRow,
    selected: bool,
    columns: Columns,
    palette: &Palette,
) -> Line<'static> {
    let key = pad(&row.entry.key, columns.key);
    let shown = display_value(&row.entry.key, &row.entry.value).replace('\n', "⏎");
    let (value, empty) = if shown.is_empty() {
        ("(empty)".to_owned(), true)
    } else {
        (shown, false)
    };
    let value = pad(&value, columns.value);
    let marker = pad(&marker(row, columns.full), columns.marker);
    let base = row_style(row, palette);
    let text = format!(
        "{} {key} {value}{}{marker}",
        scope_letter(row.entry.scope),
        if columns.marker == 0 { "" } else { " " },
    );
    if selected {
        let style = Style::new().bg(palette.selection).fg(palette.selection_fg);
        return Line::styled(pad(&text, columns.width), style);
    }
    let dim = Style::new().add_modifier(Modifier::DIM);
    let mut spans = vec![
        Span::styled(format!("{} ", scope_letter(row.entry.scope)), base),
        Span::styled(format!("{key} "), base),
        Span::styled(value, if empty { dim } else { base }),
    ];
    if columns.marker > 0 {
        spans.push(Span::styled(
            format!(" {marker}"),
            if row.shadowed { dim } else { base },
        ));
    }
    Line::from(spans)
}

/// Cell widths of the columns for one frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Columns {
    width: usize,
    key: usize,
    value: usize,
    marker: usize,
    full: bool,
}

pub(crate) fn columns(rows: &[ConfigRow], width: u16) -> Columns {
    let total = usize::from(width);
    let marker = if width >= WIDE {
        MARKER_WIDE
    } else if width >= NARROW {
        MARKER_COMPACT
    } else {
        0
    };
    let longest = rows
        .iter()
        .map(|r| UnicodeWidthStr::width(r.entry.key.as_str()))
        .max()
        .unwrap_or(0);
    let gaps = 2 + 1 + usize::from(marker > 0);
    let room = total.saturating_sub(gaps + marker);
    let key = longest.clamp(KEY_MIN, KEY_MAX).min(room / 2).max(1);
    Columns {
        width: total,
        key,
        value: room.saturating_sub(key),
        marker,
        full: width >= WIDE,
    }
}

/// The panel's title: `Git config ─ scope for changes: [L]ocal ─ 41 keys ─ filter: pull`.
pub(crate) fn title(view: &View<'_>) -> Line<'static> {
    let accent = Style::new().fg(view.palette.focus);
    let dim = Style::new().add_modifier(Modifier::DIM);
    let scope = match view.scope {
        WriteScope::Local => "[L]ocal",
        WriteScope::Global => "[G]lobal",
        WriteScope::Worktree => "[W]orktree",
    };
    let count = if view.filter.is_empty() {
        format!("{} keys", view.total)
    } else {
        format!("{} of {} keys", view.rows.len(), view.total)
    };
    let mut spans = vec![
        Span::styled(" Git config ", accent.add_modifier(Modifier::BOLD)),
        Span::styled("─ scope for changes: ", dim),
        Span::styled(scope, Style::new().fg(view.palette.key)),
        Span::styled(format!(" ─ {count}"), dim),
    ];
    if view.filtering || !view.filter.is_empty() {
        spans.push(Span::styled(" ─ filter: ", dim));
        spans.push(Span::raw(view.filter.to_owned()));
        if view.filtering {
            spans.push(Span::styled("▏", accent));
        }
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// Keep the selected item in view, with its section rule when it is the first
/// of the section, and never scroll past the last item.
pub(crate) fn scrolled(items: &[Item<'_>], selected: usize, offset: usize, height: usize) -> usize {
    let at = items
        .iter()
        .position(|i| *i == Item::Row(selected))
        .unwrap_or(0);
    let top = if at > 0 && matches!(items.get(at - 1), Some(Item::Rule(_))) {
        at - 1
    } else {
        at
    };
    let mut offset = offset;
    if top < offset {
        offset = top;
    } else if at >= offset + height {
        offset = at + 1 - height;
    }
    offset.min(items.len().saturating_sub(height))
}

/// Draw the screen into `area`. Returns the offset it settled on, for the app
/// to keep.
pub(crate) fn draw(frame: &mut Frame<'_>, area: Rect, view: &View<'_>) -> usize {
    frame.render_widget(Clear, area);
    let mut panel = Panel::new()
        .title(title(view))
        .border_style(Style::new().fg(view.palette.focus));
    if let Some(note) = view.note {
        panel = panel.bottom_title(Line::styled(
            format!(" {note} "),
            Style::new().add_modifier(Modifier::DIM),
        ));
    }
    let block = panel.block();
    let inner = block.inner(area).inner(Margin::new(1, 0));
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return view.offset;
    }

    if view.rows.is_empty() {
        let text = if view.filter.is_empty() {
            "no git config values".to_owned()
        } else {
            format!("no key or value matches \"{}\"", view.filter)
        };
        let line = Line::styled(
            cut_end(&text, usize::from(inner.width)),
            Style::new().add_modifier(Modifier::DIM),
        );
        frame.render_widget(Paragraph::new(line), inner);
        return 0;
    }

    let list = items(view.rows);
    let height = usize::from(inner.height);
    let offset = scrolled(&list, view.selected, view.offset, height);
    let cols = columns(view.rows, inner.width);
    let dim = Style::new().add_modifier(Modifier::DIM);
    let lines: Vec<Line<'static>> = list
        .iter()
        .skip(offset)
        .take(height)
        .filter_map(|item| match *item {
            Item::Rule(name) => Some(Line::styled(rule(name, cols.width), dim)),
            Item::Row(i) => view
                .rows
                .get(i)
                .map(|row| row_line(row, i == view.selected, cols, &view.palette)),
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
    ScrollBar::new(list.len(), height, offset)
        .style(Style::new().fg(view.palette.focus))
        .render(
            frame,
            Rect::new(area.right().saturating_sub(1), inner.y, 1, inner.height),
        );
    offset
}
