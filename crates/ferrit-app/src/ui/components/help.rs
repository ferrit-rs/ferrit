//! The help screen.

use crate::ui::components::keybar::help::HelpLine;
use crate::ui::components::keybar::help::filter_help_lines;
use ferrit_tui::theme::palette::Palette;
use ferrit_tui::widgets::chrome::lists::ScrollBar;
use ferrit_tui::widgets::text_input::{TextInput, TextInputMode};
use ferrit_tui::widgets::tui_overlay::anchor::Anchor;
use ferrit_tui::widgets::tui_overlay::backdrop::Backdrop;
use ferrit_tui::widgets::tui_overlay::overlay::Overlay;
use ferrit_tui::widgets::tui_overlay::slide::Slide;
use ferrit_tui::widgets::tui_overlay::state::OverlayState;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

/// Whether keys move through the help or type into its search box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HelpMode {
    Browse,
    Search,
}

pub struct HelpState {
    /// The help is up (it may still be sliding out: see `is_visible`).
    pub open: bool,
    scroll: usize,
    /// Rows the last frame showed, so a page scrolls by what is on screen.
    rows: usize,
    query: TextInput,
    mode: HelpMode,
}

impl Default for HelpState {
    fn default() -> Self {
        Self {
            open: false,
            scroll: 0,
            rows: 0,
            query: TextInput::default(),
            mode: HelpMode::Browse,
        }
    }
}

impl HelpState {
    /// Up, or still animating out.
    pub(crate) fn is_visible(&self, overlay: &OverlayState) -> bool {
        self.open || !overlay.is_closed()
    }

    /// Show the help from the top, with an empty search.
    pub(crate) fn show(&mut self) {
        self.open = true;
        self.reset();
    }

    /// Hide it, and start the slide-out.
    pub(crate) fn dismiss(&mut self, overlay: &mut OverlayState) {
        self.open = false;
        self.reset();
        overlay.close();
    }

    fn reset(&mut self) {
        self.scroll = 0;
        self.query = TextInput::default();
        self.mode = HelpMode::Browse;
    }

    /// What drawing needs: scroll, query, whether the search box has the keyboard.
    pub(crate) fn view_parts(&self) -> (usize, &TextInput, bool) {
        (self.scroll, &self.query, self.mode == HelpMode::Search)
    }

    pub(crate) fn query(&self) -> &TextInput {
        &self.query
    }

    pub(crate) fn is_searching(&self) -> bool {
        self.mode == HelpMode::Search
    }

    pub(crate) fn set_rows(&mut self, rows: usize) {
        self.rows = rows;
    }

    /// A key while the search box has the keyboard.
    pub(crate) fn search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => self.mode = HelpMode::Browse,
            _ if self.query.handle_key_event(key, TextInputMode::SingleLine) => {
                self.scroll = 0;
            },
            _ => {},
        }
    }

    pub(crate) fn start_search(&mut self) {
        self.query = TextInput::default();
        self.mode = HelpMode::Search;
        self.scroll = 0;
    }

    /// Scroll for `code` over `total` filtered lines; `false` for any other key.
    pub(crate) fn scroll_key(&mut self, code: KeyCode, total: usize) -> bool {
        let max = total.saturating_sub(self.rows.max(1));
        let page = self.rows.saturating_sub(1).max(1);
        match code {
            KeyCode::Char('j') | KeyCode::Down => self.scroll = (self.scroll + 1).min(max),
            KeyCode::Char('k') | KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = (self.scroll + page).min(max),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(page),
            KeyCode::Home | KeyCode::Char('g') => self.scroll = 0,
            KeyCode::End | KeyCode::Char('G') => self.scroll = max,
            _ => return false,
        }
        true
    }
}

/// The help screen: one line per binding of the focused pane and of the global
/// context, scrolled to `scroll`. Returns how many lines fit, so the scroll
/// keys know where the end is.
pub(crate) struct HelpView<'a> {
    pub(crate) accent: ratatui::style::Color,
    pub(crate) lines: &'a [HelpLine],
    pub(crate) scroll: usize,
    pub(crate) overlay_state: &'a mut OverlayState,
    pub(crate) query: &'a TextInput,
    pub(crate) searching: bool,
    pub(crate) palette: &'a Palette,
}

pub(crate) fn draw_help(frame: &mut Frame<'_>, area: Rect, view: HelpView<'_>) -> usize {
    let HelpView {
        accent,
        lines,
        scroll,
        overlay_state,
        query,
        searching,
        palette,
    } = view;
    let filtered = filter_help_lines(lines, &query.text());
    let width = 80.min(area.width);
    let desired_height = u16::try_from(filtered.len().saturating_add(6)).unwrap_or(u16::MAX);
    let max_height = (area.height.saturating_mul(4) / 5).max(1);
    let height = desired_height.min(max_height).min(area.height);
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(focused)
        .title(Line::styled(" keybindings ", focused));
    if overlay_state.is_closed() {
        // Keep direct render-test assignment of `show_help = true` useful;
        // real input opens the persistent state before this function runs.
        overlay_state.open();
        overlay_state.tick(std::time::Duration::from_secs(1));
    }
    frame.render_stateful_widget(
        Overlay::new()
            .anchor(Anchor::Center)
            .slide(Slide::Bottom)
            .width(Constraint::Length(width))
            .height(Constraint::Length(height))
            .backdrop(
                Backdrop::new(ratatui::style::Color::Black).fg(ratatui::style::Color::DarkGray),
            )
            .bg(ratatui::style::Color::Reset)
            .block(block),
        area,
        overlay_state,
    );
    let Some(inner) = overlay_state.inner_area() else {
        return 1;
    };
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    let [search_area, list_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(body);
    let search_style = if searching {
        focused
    } else {
        Style::new().fg(palette.idle)
    };
    let search_block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(search_style)
        .style(Style::new().bg(palette.focus_box))
        .title(Line::styled(" SEARCH COMMAND ", search_style));
    let input_area = search_block.inner(search_area);
    frame.render_widget(search_block, search_area);
    let [label_area, value_area] =
        Layout::horizontal([Constraint::Length(9), Constraint::Min(0)]).areas(input_area);
    frame.render_widget(
        Paragraph::new(Line::styled("query: /", Style::new().fg(palette.key))),
        label_area,
    );
    if searching {
        query.render(frame, value_area);
    } else {
        query.render_inactive(frame, value_area);
    }

    let rows = usize::from(list_area.height);
    let max = filtered.len().saturating_sub(rows.max(1));
    let start = scroll.min(max);
    let key_width = filtered
        .iter()
        .filter_map(|line| match line {
            HelpLine::Entry { keys, .. } => Some(keys.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        .min(24);
    let rendered: Vec<Line<'static>> = filtered
        .iter()
        .skip(start)
        .take(rows)
        .map(|line| match line {
            HelpLine::Heading(text) => Line::styled(
                text.clone(),
                Style::new().fg(accent).add_modifier(Modifier::BOLD),
            ),
            HelpLine::Entry { keys, text } => Line::from(vec![
                Span::styled(
                    format!("{keys:<key_width$}  "),
                    Style::new().fg(palette.key),
                ),
                Span::raw(text.clone()),
            ]),
            HelpLine::Blank => Line::raw(""),
        })
        .collect();
    let [text_area, bar_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(list_area);
    let rendered = if rendered.is_empty() {
        vec![Line::styled(
            format!("no command matches {:?}", query.text()),
            Style::new().fg(palette.idle),
        )]
    } else {
        rendered
    };
    frame.render_widget(Paragraph::new(rendered), text_area);
    ScrollBar::new(filtered.len(), rows, start)
        .style(Style::new().fg(palette.idle))
        .render(frame, bar_area);
    let position = if max == 0 {
        String::new()
    } else {
        format!(
            " \u{b7} {}/{}",
            start + rows.min(filtered.len()),
            filtered.len()
        )
    };
    let footer_text = if searching {
        "type to filter \u{b7} enter apply \u{b7} esc cancel"
    } else {
        " / search \u{b7} j/k scroll \u{b7} ?/esc close"
    };
    frame.render_widget(
        Paragraph::new(Line::styled(
            format!("{footer_text}{position}"),
            Style::new().fg(palette.idle),
        )),
        footer,
    );
    rows
}
