use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Multi-select rows. The parent owns selected values and cursor movement.
#[must_use]
pub struct MultiSelect<'a> {
    items: &'a [Line<'a>],
    selected: &'a [bool],
    cursor: usize,
    style: Style,
    selected_style: Style,
    cursor_style: Style,
}

impl<'a> MultiSelect<'a> {
    pub fn new(items: &'a [Line<'a>], selected: &'a [bool], cursor: usize) -> Self {
        Self {
            items,
            selected,
            cursor,
            style: Style::default(),
            selected_style: Style::default(),
            cursor_style: Style::default(),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn selected_style(mut self, style: Style) -> Self {
        self.selected_style = style;
        self
    }

    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }

    pub fn lines(&self) -> Vec<Line<'a>> {
        self.items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let checked = self.selected.get(index).copied().unwrap_or(false);
                let marker = if checked { "[x] " } else { "[ ] " };
                let style = if index == self.cursor {
                    self.cursor_style
                } else if checked {
                    self.selected_style
                } else {
                    self.style
                };
                let mut line = item.clone().patch_style(style);
                line.spans.insert(0, Span::styled(marker, style));
                line
            })
            .collect()
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.lines()), area);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;

    use crate::widgets::multi_select::MultiSelect;

    #[test]
    fn multi_select_marks_checked_rows() {
        let items = vec![Line::from("One"), Line::from("Two")];
        let selected = [true, false];
        let lines = MultiSelect::new(&items, &selected, 1).lines();
        let rendered = lines.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(rendered, ["[x] One", "[ ] Two"]);
    }
}
