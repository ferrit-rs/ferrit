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

use crate::components::ui::cut::cut_middle;
use crate::components::ui::dialog::Dialog;
use crate::components::ui::palette::Palette;

/// The dialog is never wider than this.
const WIDTH: u16 = 64;
/// Rows of text inside the dialog.
const ROWS: u16 = 6;
/// Cells of margin between the border and the text.
const PAD: usize = 2;

/// What the screen draws.
#[derive(Debug)]
pub struct View<'a> {
    pub dir: &'a Path,
    pub palette: Palette,
    pub accent: ratatui::style::Color,
}

/// One row of the menu: `▸ i   Initialise…` or `  q   Quit`.
fn choice(marker: &str, key: &str, text: &str, palette: &Palette) -> Line<'static> {
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
pub fn draw(frame: &mut Frame<'_>, area: Rect, view: &View<'_>) {
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
            "\u{25b8}",
            "i",
            "Initialise a repository here (git init)",
            &view.palette,
        ),
        choice(" ", "q", "Quit", &view.palette),
    ];
    frame.render_widget(Paragraph::new(lines), dialog.body);
}
