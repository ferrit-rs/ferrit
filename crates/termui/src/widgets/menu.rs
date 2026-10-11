use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Vertical menu with a parent-owned selected index.
#[must_use]
pub struct Menu<'a> {
    items: &'a [Line<'a>],
    selected: usize,
    style: Style,
    selected_style: Style,
    marker: &'a str,
}

impl<'a> Menu<'a> {
    pub fn new(items: &'a [Line<'a>], selected: usize) -> Self {
        Self {
            items,
            selected,
            style: Style::default(),
            selected_style: Style::default()
                .fg(Color::Rgb(96, 165, 250))
                .add_modifier(Modifier::BOLD),
            marker: "› ",
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

    pub fn marker(mut self, marker: &'a str) -> Self {
        self.marker = marker;
        self
    }

    pub fn lines(&self) -> Vec<Line<'a>> {
        self.items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let selected = index == self.selected;
                let style = if selected {
                    self.selected_style
                } else {
                    self.style
                };
                let marker = if selected { self.marker } else { "  " };
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

    use crate::widgets::menu::Menu;

    #[test]
    fn menu_marks_selected_item() {
        let items = vec![Line::from("Home"), Line::from("Settings")];
        let lines = Menu::new(&items, 1).lines();
        let rendered = lines.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(rendered, ["  Home", "› Settings"]);
    }
}
