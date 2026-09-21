//! Bordered radio option card for compact choice panels.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

const CARD_BORDER_WIDTH: usize = 2;
const CARD_CONTENT_PADDING: &str = " ";
const CARD_SELECTED_MARK: &str = "◉";
const CARD_UNSELECTED_MARK: &str = "○";
const CARD_TOP_LEFT: &str = "┌";
const CARD_TOP_RIGHT: &str = "┐";
const CARD_BOTTOM_LEFT: &str = "└";
const CARD_BOTTOM_RIGHT: &str = "┘";
const CARD_HORIZONTAL: &str = "─";
const CARD_VERTICAL: &str = "│";
const DEFAULT_ACCENT: Color = Color::Green;

#[derive(Debug, Clone)]
pub struct RadioCard {
    key: String,
    title: String,
    description: String,
    selected: bool,
    accent: Color,
}

impl RadioCard {
    pub const HEIGHT: usize = 4;

    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: String::new(),
            title: title.into(),
            description: description.into(),
            selected: false,
            accent: DEFAULT_ACCENT,
        }
    }

    #[must_use]
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    #[must_use]
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    #[must_use]
    pub fn accent(mut self, accent: Color) -> Self {
        self.accent = accent;
        self
    }

    pub fn lines(self, width: u16) -> Vec<Line<'static>> {
        let width = usize::from(width);
        let border_style = Style::new().fg(if self.selected {
            self.accent
        } else {
            Color::DarkGray
        });
        let title_style = Style::new()
            .fg(if self.selected {
                self.accent
            } else {
                Color::Reset
            })
            .add_modifier(if self.selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        let horizontal_count = width.saturating_sub(CARD_BORDER_WIDTH);
        let border = CARD_HORIZONTAL.repeat(horizontal_count);
        let top = Line::from(vec![
            Span::styled(CARD_TOP_LEFT, border_style),
            Span::styled(border.clone(), border_style),
            Span::styled(CARD_TOP_RIGHT, border_style),
        ]);
        let title = content_line(
            &self.title,
            &format!(
                "{}{}",
                self.key,
                if self.key.is_empty() { "" } else { " · " }
            ),
            if self.selected {
                CARD_SELECTED_MARK
            } else {
                CARD_UNSELECTED_MARK
            },
            width,
            title_style,
            border_style,
        );
        let description = content_line(
            &self.description,
            CARD_CONTENT_PADDING,
            "",
            width,
            Style::new().fg(Color::Gray),
            border_style,
        );
        let bottom = Line::from(vec![
            Span::styled(CARD_BOTTOM_LEFT, border_style),
            Span::styled(border, border_style),
            Span::styled(CARD_BOTTOM_RIGHT, border_style),
        ]);
        vec![top, title, description, bottom]
    }
}

fn content_line(
    value: &str,
    prefix: &str,
    suffix: &str,
    width: usize,
    content_style: Style,
    border_style: Style,
) -> Line<'static> {
    let inner_width = width.saturating_sub(CARD_BORDER_WIDTH);
    let prefix_width = UnicodeWidthStr::width(prefix);
    let suffix_width = UnicodeWidthStr::width(suffix);
    let available = inner_width.saturating_sub(prefix_width.saturating_add(suffix_width));
    let value = fit_width(value, available);
    let used = prefix_width
        .saturating_add(UnicodeWidthStr::width(value.as_str()))
        .saturating_add(suffix_width);
    let padding = " ".repeat(inner_width.saturating_sub(used));
    Line::from(vec![
        Span::styled(CARD_VERTICAL, border_style),
        Span::styled(prefix.to_owned(), content_style),
        Span::styled(value, content_style),
        Span::raw(padding),
        Span::styled(suffix.to_owned(), content_style),
        Span::styled(CARD_VERTICAL, border_style),
    ])
}

fn fit_width(value: &str, width: usize) -> String {
    let mut fitted = String::new();
    let mut used = 0_usize;
    for character in value.chars() {
        let char_width = unicode_width::UnicodeWidthChar::width(character).unwrap_or(0);
        if used.saturating_add(char_width) > width {
            break;
        }
        fitted.push(character);
        used = used.saturating_add(char_width);
    }
    fitted
}
