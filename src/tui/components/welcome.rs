//! The welcome screen for a folder without a repository.

use crate::theme::palette::Palette;
use crate::tui::App;
use crate::tui::components::keybar::draw_keybar;
use crate::tui::components::popups::{ConfirmAction, ConfirmPrompt};
use crate::tui::draw::TAGLINE_PROMISE;
use crate::tui::draw::TAGLINE_WHAT;
use crate::tui::draw::{Landed, RenderState};
use crate::tui::event::Event;
use crate::tui::widgets::cut::cut_middle;
use crate::tui::widgets::dialog::Dialog;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use std::path::Path;
use unicode_width::UnicodeWidthStr;

const WELCOME_ROWS: usize = 2;

/// `j` / `k` move the choice, `i` or Enter on the first row asks to `git init`,
/// Enter on the second row, `q` and Esc quit.
pub(crate) fn key(selected: usize, dir: Option<&Path>, key: KeyEvent) -> Vec<Event> {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') | KeyCode::End => {
            vec![Event::WelcomeSelected((selected + 1).min(WELCOME_ROWS - 1))]
        },
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Home => {
            vec![Event::WelcomeSelected(selected.saturating_sub(1))]
        },
        KeyCode::Enter if selected == 0 => ask_init(dir),
        KeyCode::Enter | KeyCode::Char('q') | KeyCode::Esc => vec![Event::Quit],
        KeyCode::Char('i') => ask_init(dir),
        _ => Vec::new(),
    }
}

/// Ask before `git init`; the home folder gets a louder question.
fn ask_init(dir: Option<&Path>) -> Vec<Event> {
    let Some(dir) = dir else {
        return Vec::new();
    };
    let home = std::env::var_os("HOME").is_some_and(|home| Path::new(&home) == dir);
    let message = if home {
        format!(
            "run git init in {}? This is your home folder.",
            dir.display()
        )
    } else {
        format!("run git init in {}?", dir.display())
    };
    vec![Event::Ask(ConfirmPrompt {
        message,
        action: ConfirmAction::InitRepo(dir.to_path_buf()),
    })]
}

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

/// Status pane's right side: lazygit's welcome screen, not a repo-status
/// view (see `docs/PLAN_1_LAYOUT.md`, "Welcome screen"). No repo data, so
/// this renders identically in `App::mock()` and against a real repo. Below
/// every tier's minimum area, the wordmark is dropped for a plain `ferrit`
/// label instead of wrapping into noise.
pub(crate) fn welcome_lines(
    width: u16,
    height: u16,
    accent: ratatui::style::Color,
    palette: &Palette,
) -> Vec<Line<'static>> {
    let fits = |w: &Wordmark| width >= w.min_area.0 && height >= w.min_area.1;
    let wordmark = [WORDMARK_LARGE, WORDMARK_MEDIUM, WORDMARK_SMALL]
        .into_iter()
        .find(fits);

    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(wordmark) = wordmark {
        lines.extend(wordmark.art.lines().map(|line| {
            let padded = format!("{line:<0$}", wordmark.width);
            Line::styled(padded, Style::new().fg(accent)).centered()
        }));
    } else {
        lines.push(
            Line::styled(
                "ferrit",
                Style::new().fg(accent).add_modifier(Modifier::BOLD),
            )
            .centered(),
        );
    }
    lines.push(Line::raw(""));
    lines.push(
        Line::styled(
            TAGLINE_WHAT,
            Style::new().fg(accent).add_modifier(Modifier::BOLD),
        )
        .centered(),
    );
    lines.push(Line::raw(TAGLINE_PROMISE).centered());
    lines.push(Line::raw(""));
    let idle = Style::new().fg(palette.idle);
    lines.push(
        Line::styled(
            format!(
                "v{} \u{b7} {} \u{b7} {}",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_LICENSE"),
                env!("CARGO_PKG_AUTHORS"),
            ),
            idle,
        )
        .centered(),
    );
    lines.push(Line::styled(env!("CARGO_PKG_REPOSITORY"), idle).centered());
    lines.push(Line::raw(""));
    lines.push(Line::styled("Press ? for keybindings", idle).centered());
    lines
}

/// The welcome screen above its key bar; returns the key bar's area.
pub(crate) fn draw_welcome(
    frame: &mut Frame<'_>,
    app: &App,
    render: &RenderState,
    landed: &mut Landed,
    area: Rect,
) -> Rect {
    let [page, keybar] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    if let Some(dir) = app.welcome_dir() {
        let view = View {
            dir,
            selected: app.welcome_selected(),
            palette: app.palette(),
            accent: app.theme.config.color(),
        };
        draw(frame, page, &view);
    }
    draw_keybar(frame, keybar, app, render, landed);
    keybar
}

/// Three `ferrit` wordmarks, generated with `toilet` rather than
/// hand-drawn, so the glyphs are guaranteed to line up — lazygit grows its
/// own banner as the terminal grows rather than showing one fixed size, so
/// `welcome_lines` picks the biggest of these three that still fits instead
/// of the single fixed logo the first attempt at this used. Each row is
/// trimmed of the trailing blank columns `toilet` pads it to (avoids
/// trailing whitespace in the source); `welcome_lines` re-pads every row to
/// its tier's width before centering it, since figlet-style fonts rely on
/// every row spanning the same width — letting each row's own (different)
/// trimmed length drive `Line::centered()` would shift rows against each
/// other and break the letterforms.
pub(crate) struct Wordmark {
    art: &'static str,
    width: usize,
    /// Right-pane `(width, height)` needed to show this tier without
    /// clipping or crowding the text below it.
    min_area: (u16, u16),
}

/// `toilet -f smmono12 ferrit`.
pub(crate) const WORDMARK_SMALL: Wordmark = Wordmark {
    art: "\
  ▄▄                  █
 ▐▛▀                  ▀   ▐▌
▐███  ▟█▙  █▟█▌ █▟█▌ ██  ▐███
 ▐▌  ▐▙▄▟▌ █▘   █▘    █   ▐▌
 ▐▌  ▐▛▀▀▘ █    █     █   ▐▌
 ▐▌  ▝█▄▄▌ █    █   ▗▄█▄▖ ▐▙▄
 ▝▘   ▝▀▀  ▀    ▀   ▝▀▀▀▘  ▀▀",
    width: 30,
    min_area: (40, 16),
};
/// `toilet -f mono12 ferrit`. Same width as `WORDMARK_LARGE` (both are a
/// 60-column canvas) — what changes between the two is height, not width.
pub(crate) const WORDMARK_MEDIUM: Wordmark = Wordmark {
    art: "\
    ▄▄▄▄                                    ██
   ██▀▀▀                                    ▀▀       ██
 ███████    ▄████▄    ██▄████   ██▄████   ████     ███████
   ██      ██▄▄▄▄██   ██▀       ██▀         ██       ██
   ██      ██▀▀▀▀▀▀   ██        ██          ██       ██
   ██      ▀██▄▄▄▄█   ██        ██       ▄▄▄██▄▄▄    ██▄▄▄
   ▀▀        ▀▀▀▀▀    ▀▀        ▀▀       ▀▀▀▀▀▀▀▀     ▀▀▀▀",
    width: 60,
    min_area: (70, 16),
};
/// `toilet -f bigmono12 ferrit`: same canvas width as medium, but almost
/// twice the rows — denser and bolder rather than wider, so it needs
/// extra height more than extra width.
pub(crate) const WORDMARK_LARGE: Wordmark = Wordmark {
    art: "\
                                            ██
   ▒████                                    ██
   █████                                    ██       ██
   ██                                                ██
 ███████    ░████▒    ██░████   ██░████   ████     ███████
 ███████   ░██████▒   ███████   ███████   ████     ███████
   ██      ██▒  ▒██   ███░      ███░        ██       ██
   ██      ████████   ██        ██          ██       ██
   ██      ████████   ██        ██          ██       ██
   ██      ██         ██        ██          ██       ██
   ██      ███░  ▒█   ██        ██          ██       ██░
   ██      ░███████   ██        ██       ████████    █████
   ██       ░█████▒   ██        ██       ████████    ░████",
    width: 60,
    min_area: (70, 24),
};
