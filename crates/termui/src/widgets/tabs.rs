use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Tab row. Selection movement stays owned by the parent app.
#[must_use]
pub struct Tabs<'a> {
    items: &'a [Line<'a>],
    selected: usize,
    style: Style,
    selected_style: Style,
    divider: &'a str,
}

impl<'a> Tabs<'a> {
    pub fn new(items: &'a [Line<'a>], selected: usize) -> Self {
        Self {
            items,
            selected,
            style: Style::default(),
            selected_style: Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            divider: "  ",
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

    pub fn divider(mut self, divider: &'a str) -> Self {
        self.divider = divider;
        self
    }

    pub fn line(&self) -> Line<'a> {
        let mut spans = Vec::new();
        for (index, item) in self.items.iter().enumerate() {
            if index > 0 {
                spans.push(Span::raw(self.divider));
            }
            let style = if index == self.selected {
                self.selected_style
            } else {
                self.style
            };
            spans.extend(item.clone().patch_style(style).spans);
        }
        Line::from(spans)
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line()), area);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;

    use crate::widgets::tabs::Tabs;

    #[test]
    fn tabs_join_items_and_dividers() {
        let items = vec![Line::from("Overview"), Line::from("Commits")];
        assert_eq!(Tabs::new(&items, 1).line().to_string(), "Overview  Commits");
    }

    #[test]
    fn out_of_range_selection_keeps_all_tabs_unselected() {
        let items = vec![Line::from("Overview")];
        assert_eq!(Tabs::new(&items, 4).line().to_string(), "Overview");
    }
}
