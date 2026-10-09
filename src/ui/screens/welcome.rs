//! The welcome screen (`docs/PLAN_16_START_WITHOUT_REPO.md`): one dialog in the
//! middle of the screen that names the folder, says it is not a repository, and
//! lists the two things that can be done. A pure function of the folder and the
//! colours.

use std::path::Path;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::theme::palette::Palette;
use crate::ui::widgets::cut::cut_middle;
use crate::ui::widgets::dialog::Dialog;

/// The dialog is never wider than this.
const WIDTH: u16 = 64;
/// Rows of text inside the dialog.
const ROWS: u16 = 6;
/// Cells of margin between the border and the text.
const PAD: usize = 2;

/// What the screen draws.
#[derive(Debug)]
pub(crate) struct View<'a> {
    pub dir: &'a Path,
    /// The highlighted row: 0 is `git init`, 1 is quit.
    pub selected: usize,
    pub palette: Palette,
    pub accent: ratatui::style::Color,
}

/// One row of the menu: `▸ i   Initialise…` or `  q   Quit`. The highlighted
/// row has the marker and the selection bar of the other menus, across the
/// dialog.
fn choice(selected: bool, key: &str, text: &str, width: usize, palette: &Palette) -> Line<'static> {
    let marker = if selected { "\u{25b8}" } else { " " };
    if selected {
        let row = format!("{}{marker} {key}   {text}", " ".repeat(PAD));
        let used = UnicodeWidthStr::width(row.as_str());
        let padded = format!("{row}{}", " ".repeat(width.saturating_sub(used)));
        return Line::styled(
            padded,
            Style::new()
                .fg(palette.selection_fg)
                .bg(palette.selection)
                .add_modifier(Modifier::BOLD),
        );
    }
    Line::from(vec![
        Span::raw(format!("{}{marker} ", " ".repeat(PAD))),
        Span::styled(
            key.to_owned(),
            Style::new().fg(palette.key).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("   {text}")),
    ])
}

/// Draw the dialog centred in `area`.
pub(crate) fn draw(frame: &mut Frame<'_>, area: Rect, view: &View<'_>) {
    frame.render_widget(Clear, area);
    let accent = Style::new().fg(view.accent).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(" ferrit ", accent))
        .fit_content(WIDTH.min(area.width), ROWS, 0)
        .border_style(accent)
        .render(frame, area);
    let room = usize::from(dialog.body.width).saturating_sub(PAD * 2);
    let folder = cut_middle(&view.dir.display().to_string(), room);
    let lines = vec![
        Line::raw(""),
        Line::styled(
            format!("{}{folder}", " ".repeat(PAD)),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!("{}is not a git repository.", " ".repeat(PAD))),
        Line::raw(""),
        choice(
            view.selected == 0,
            "i",
            "Initialise a repository here (git init)",
            usize::from(dialog.body.width),
            &view.palette,
        ),
        choice(
            view.selected == 1,
            "q",
            "Quit",
            usize::from(dialog.body.width),
            &view.palette,
        ),
    ];
    frame.render_widget(Paragraph::new(lines), dialog.body);
}
