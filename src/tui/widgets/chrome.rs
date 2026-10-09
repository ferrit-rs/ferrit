//! chrome.rs

use crate::theme::palette::Palette;
use crate::tui::widgets::tui_overlay::anchor::Anchor;
use crate::tui::widgets::tui_overlay::backdrop::Backdrop;
use crate::tui::widgets::tui_overlay::overlay::Overlay;
use crate::tui::widgets::tui_overlay::slide::Slide;
use crate::tui::widgets::tui_overlay::state::OverlayState;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Clear, List, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState,
};
use std::io::{self, Write};
use unicode_width::UnicodeWidthStr;

/// Ferrit's shared key hints and inline confirmation prompt renderer.
pub struct KeyBar(Line<'static>);

impl KeyBar {
    pub fn hints(raw: &str, palette: &Palette) -> Self {
        Self(crate::theme::palette::keybar_line(raw, palette))
    }

    pub fn confirm(message: &str, palette: &Palette) -> Self {
        Self(crate::theme::palette::confirm_line(message, palette))
    }

    pub fn line(self) -> Line<'static> {
        self.0
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line()), area);
    }
}

/// Ferrit's shared rounded panel shell, ready to attach to any Ratatui widget.
#[derive(Default)]
#[must_use]
pub(crate) struct Panel<'a> {
    title: Option<Line<'a>>,
    bottom_title: Option<Line<'a>>,
    border_style: Style,
}

impl<'a> Panel<'a> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn title<T: Into<Line<'a>>>(mut self, title: T) -> Self {
        self.title = Some(title.into());
        self
    }

    pub(crate) fn bottom_title<T: Into<Line<'a>>>(mut self, title: T) -> Self {
        self.bottom_title = Some(title.into());
        self
    }

    pub(crate) fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    pub(crate) fn block(self) -> Block<'a> {
        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.border_style);
        if let Some(title) = self.title {
            block = block.title(title);
        }
        if let Some(title) = self.bottom_title {
            block = block.title_bottom(title);
        }
        block
    }
}

/// Vertical scrollbar shared by pane lists and scrollable previews.
#[must_use]
pub(crate) struct ScrollBar {
    content_length: usize,
    viewport_length: usize,
    position: usize,
    style: Style,
}

impl ScrollBar {
    pub(crate) fn new(content_length: usize, viewport_length: usize, position: usize) -> Self {
        Self {
            content_length,
            viewport_length,
            position,
            style: Style::default(),
        }
    }

    pub(crate) fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Draw only when content exceeds viewport. `area` is the scrollbar track.
    pub(crate) fn render(self, frame: &mut Frame<'_>, area: Rect) {
        if self.content_length <= self.viewport_length {
            return;
        }

        let max_scroll = self.content_length - self.viewport_length;
        let mut state = ScrollbarState::new(max_scroll + 1)
            .position(self.position.min(max_scroll))
            .viewport_content_length(self.viewport_length);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(self.style)
                .begin_symbol(None)
                .end_symbol(None),
            area,
            &mut state,
        );
    }
}

/// Render selectable rows with a full-width style on the active row.
/// Selection movement remains owned by the parent app.
#[must_use]
pub(crate) struct SelectList<'a> {
    items: &'a [Line<'static>],
    selected: usize,
    selection_style: Style,
}

impl<'a> SelectList<'a> {
    pub(crate) fn new(items: &'a [Line<'static>], selected: usize) -> Self {
        Self {
            items,
            selected,
            selection_style: Style::default(),
        }
    }

    pub(crate) fn selection_style(mut self, style: Style) -> Self {
        self.selection_style = style;
        self
    }

    pub(crate) fn render(self, frame: &mut Frame<'_>, area: Rect) {
        let lines = self
            .items
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, mut line)| {
                if index == self.selected {
                    let padding = (area.width as usize).saturating_sub(line.width());
                    if padding > 0 {
                        line.spans.push(Span::raw(" ".repeat(padding)));
                    }
                    for span in &mut line.spans {
                        span.style = self.selection_style;
                    }
                }
                line
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), area);
    }
}

const ZERO_WIDTH: usize = 0;
const LABEL_PADDING_CELLS: usize = 2;
const CENTER_SPLIT_DIVISOR: usize = 2;
const HORIZONTAL_MARGIN_MULTIPLIER: usize = 2;

/// Horizontal divider, optionally labeled. Can render alone or join a
/// scrollable `Paragraph` as a line through [`Separator::line`].
#[derive(Debug, Clone)]
pub(crate) struct Separator {
    label: String,
    style: Style,
    glyph: char,
    margin_x: u16,
}

impl Separator {
    pub(crate) fn new<L: Into<String>>(label: L) -> Self {
        Self {
            label: label.into(),
            style: Style::new().fg(Color::DarkGray),
            glyph: '─',
            margin_x: 0,
        }
    }

    #[must_use]
    pub(crate) fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set equal left and right margins, measured in terminal cells.
    #[must_use]
    pub(crate) fn margin_x(mut self, cells: u16) -> Self {
        self.margin_x = cells;
        self
    }

    /// Build divider line sized to terminal cells, suitable for scrollable text.
    pub(crate) fn line(&self, width: u16) -> Line<'static> {
        let margin = usize::from(self.margin_x);
        let width = usize::from(width);
        let content_width = width.saturating_sub(margin * HORIZONTAL_MARGIN_MULTIPLIER);
        let mut line = self.content_line(content_width);
        if margin > ZERO_WIDTH {
            line.spans.insert(ZERO_WIDTH, Span::raw(" ".repeat(margin)));
        }
        line
    }

    fn content_line(&self, width: usize) -> Line<'static> {
        if width == ZERO_WIDTH {
            return Line::default();
        }
        let label = self.label.trim();
        if label.is_empty() {
            return Line::styled(self.glyph.to_string().repeat(width), self.style);
        }

        let label_width = Line::from(label.to_owned()).width();
        if label_width + LABEL_PADDING_CELLS >= width {
            return Line::styled(label.to_owned(), self.style);
        }
        let remaining = width - label_width - LABEL_PADDING_CELLS;
        let left = remaining / CENTER_SPLIT_DIVISOR;
        let right = remaining - left;
        Line::from(vec![
            Span::styled(self.glyph.to_string().repeat(left), self.style),
            Span::styled(format!(" {label} "), self.style),
            Span::styled(self.glyph.to_string().repeat(right), self.style),
        ])
    }
}

/// Right-side drawer shell. Returns its inner area for caller-owned content.
#[must_use]
pub(crate) struct Drawer<'state, 'title> {
    state: &'state mut OverlayState,
    title: Line<'title>,
    width: Constraint,
    border_style: Style,
}

impl<'state, 'title> Drawer<'state, 'title> {
    pub(crate) fn new<T: Into<Line<'title>>>(state: &'state mut OverlayState, title: T) -> Self {
        Self {
            state,
            title: title.into(),
            width: Constraint::Percentage(50),
            border_style: Style::new(),
        }
    }

    pub(crate) fn width(mut self, width: Constraint) -> Self {
        self.width = width;
        self
    }

    pub(crate) fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Draw backdrop and frame, then return the area for drawer body widgets.
    pub(crate) fn render(self, frame: &mut Frame<'_>, area: Rect) -> Option<Rect> {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.border_style)
            .title(self.title);
        let overlay = Overlay::new()
            .anchor(Anchor::Right)
            .slide(Slide::Right)
            .width(self.width)
            .height(Constraint::Percentage(100))
            .backdrop(Backdrop::new(Color::Black).fg(Color::DarkGray))
            .block(block);
        frame.render_stateful_widget(overlay, area, self.state);
        self.state.inner_area()
    }
}

/// `text` cut to `width` cells with a trailing `…`.
pub(crate) fn cut_end(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    let keep: String = take_width(text.chars(), width.saturating_sub(1));
    if width == 0 {
        keep
    } else {
        format!("{keep}…")
    }
}

/// `text` cut in the middle (`src/app/…/mod.rs`): the end of a path is the part
/// that tells files apart, so it keeps the larger share.
pub(crate) fn cut_middle(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    if width <= 1 {
        return cut_end(text, width);
    }
    let room = width - 1;
    let head = take_width(text.chars(), room / 3);
    let mut tail: Vec<char> = Vec::new();
    let mut used = 0;
    for c in text.chars().rev() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > room - room / 3 {
            break;
        }
        used += w;
        tail.push(c);
    }
    tail.reverse();
    format!("{head}…{}", tail.into_iter().collect::<String>())
}

fn take_width(chars: impl Iterator<Item = char>, width: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in chars {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > width {
            break;
        }
        used += w;
        out.push(c);
    }
    out
}

/// Rendered regions inside a dialog shell.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DialogAreas {
    pub body: Rect,
    /// Empty rectangle when this dialog has no footer region.
    pub footer: Rect,
}

/// Centered, bordered overlay shell. Render body and footer widgets into the
/// returned areas to compose dialogs without coupling them to app state.
#[must_use]
pub(crate) struct Dialog<'a> {
    title: Line<'a>,
    width: u16,
    height: u16,
    footer_rows: Option<u16>,
    border_style: Style,
}

impl<'a> Dialog<'a> {
    pub(crate) fn new<T: Into<Line<'a>>>(title: T) -> Self {
        Self {
            title: title.into(),
            width: u16::MAX,
            height: u16::MAX,
            footer_rows: None,
            border_style: Style::default(),
        }
    }

    pub(crate) fn size(mut self, width: u16, height: u16) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Size the shell to its content rows plus borders, clamped to the
    /// available terminal area at render time. `footer_rows == 0` omits the
    /// footer region. Use this for dialogs whose content determines height.
    pub(crate) fn fit_content(mut self, width: u16, body_rows: u16, footer_rows: u16) -> Self {
        self.width = width;
        self.height = body_rows.saturating_add(footer_rows).saturating_add(2);
        self.footer_rows = (footer_rows > 0).then_some(footer_rows);
        self
    }

    pub(crate) fn footer_rows(mut self, rows: u16) -> Self {
        self.footer_rows = Some(rows);
        self
    }

    pub(crate) fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Clear the overlay area, draw its shell, and return child regions.
    pub(crate) fn render(self, frame: &mut Frame<'_>, area: Rect) -> DialogAreas {
        let width = self.width.min(area.width);
        let height = self.height.min(area.height);
        let outer = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y: area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        };
        let block = Panel::new()
            .title(self.title)
            .border_style(self.border_style)
            .block();
        let inner = block.inner(outer);

        frame.render_widget(Clear, outer);
        frame.render_widget(block, outer);

        let (body, footer) = match self.footer_rows {
            Some(rows) => Layout::vertical([Constraint::Min(1), Constraint::Length(rows)])
                .areas(inner)
                .into(),
            None => (inner, Rect::default()),
        };
        DialogAreas { body, footer }
    }
}

#[must_use]
pub(crate) struct PaneList<'a> {
    items: Vec<Line<'static>>,
    block: Block<'a>,
    selected: Option<usize>,
    offset: usize,
    detached: bool,
    highlight_style: Style,
    scrollbar_style: Style,
}

impl<'a> PaneList<'a> {
    pub(crate) fn new(items: Vec<Line<'static>>, block: Block<'a>) -> Self {
        Self {
            items,
            block,
            selected: None,
            offset: 0,
            detached: false,
            highlight_style: Style::default(),
            scrollbar_style: Style::default(),
        }
    }

    pub(crate) fn selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub(crate) fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// The view was scrolled on its own: keep `offset` (down to the last full
    /// page) even when `selected` is off screen, and draw no highlight then.
    pub(crate) fn detached(mut self, detached: bool) -> Self {
        self.detached = detached;
        self
    }

    pub(crate) fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    pub(crate) fn scrollbar_style(mut self, style: Style) -> Self {
        self.scrollbar_style = style;
        self
    }

    pub(crate) fn render(self, frame: &mut Frame<'_>, area: Rect) -> usize {
        let viewport_height = self.block.inner(area).height as usize;
        let row_count = self.items.len();
        let offset = if self.detached {
            self.offset.min(row_count.saturating_sub(viewport_height))
        } else {
            self.offset
        };
        let mut state = ListState::default().with_offset(offset);
        if let Some(selected) = self.selected.filter(|_| row_count > 0) {
            let selected = selected.min(row_count - 1);
            // Never `select(None)`: ratatui resets the offset to 0 when it is.
            if !self.detached || (offset..offset + viewport_height).contains(&selected) {
                state.select(Some(selected));
            }
        }

        let list = List::new(self.items)
            .block(self.block)
            .highlight_style(self.highlight_style);
        frame.render_stateful_widget(list, area, &mut state);
        ScrollBar::new(row_count, viewport_height, state.offset())
            .style(self.scrollbar_style)
            .render(
                frame,
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
            );

        state.offset()
    }
}

/// Sets the terminal's native mouse pointer shape for hoverable controls.
/// Terminals without OSC 22 support ignore these sequences.
#[derive(Debug, Default)]
pub(crate) struct MousePointer {
    /// What the hover logic last asked for.
    wanted: bool,
    /// What the terminal is currently showing.
    is_hand: bool,
}

impl MousePointer {
    /// Record whether the pointer is over an interactive target. Nothing is
    /// written until `sync`.
    pub(crate) fn request(&mut self, hovered: bool) {
        self.wanted = hovered;
    }

    /// Show a pointing hand while the last `request` was `true`, restoring
    /// the terminal default when the pointer leaves the target. Writes only
    /// on a change.
    pub(crate) fn sync(&mut self) -> io::Result<()> {
        if self.is_hand == self.wanted {
            return Ok(());
        }
        Self::write_shape(self.wanted)?;
        self.is_hand = self.wanted;
        Ok(())
    }

    /// Reset OSC 22 state when Ferrit starts or restores the terminal.
    pub(crate) fn reset_terminal() -> io::Result<()> {
        Self::write_shape(false)
    }

    fn write_shape(hand: bool) -> io::Result<()> {
        let sequence = if hand {
            b"\x1b]22;pointer\x1b\\".as_slice()
        } else {
            b"\x1b]22;\x1b\\".as_slice()
        };
        let mut stdout = io::stdout().lock();
        stdout.write_all(sequence)?;
        stdout.flush()
    }
}

#[cfg(test)]
mod tests_cut {
    use super::*;

    #[test]
    fn cuts() {
        assert_eq!(cut_end("hello", 5), "hello");
        assert_eq!(cut_end("hello world", 6), "hello…");
        assert_eq!(cut_end("hello", 0), "");
        assert_eq!(cut_middle("src/app.rs", 20), "src/app.rs");
        let cut = cut_middle("src/app/screens/dashboard/mod.rs", 20);
        assert_eq!(UnicodeWidthStr::width(cut.as_str()), 20);
        assert!(cut.starts_with("src/") && cut.ends_with("/mod.rs") && cut.contains('…'));
        assert_eq!(cut_middle("abcdef", 1), "…");
        assert_eq!(cut_middle("abcdef", 2), "…f");
    }
}
