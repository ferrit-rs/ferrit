use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Horizontal navigation path with an emphasized final item.
#[must_use]
pub struct Breadcrumbs<'a> {
    items: &'a [Line<'a>],
    separator: &'a str,
    style: Style,
    current_style: Style,
}

impl<'a> Breadcrumbs<'a> {
    pub fn new(items: &'a [Line<'a>]) -> Self {
        Self {
            items,
            separator: " / ",
            style: Style::default(),
            current_style: Style::default().add_modifier(Modifier::BOLD),
        }
    }

    pub fn separator(mut self, separator: &'a str) -> Self {
        self.separator = separator;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn current_style(mut self, style: Style) -> Self {
        self.current_style = style;
        self
    }

    pub fn line(&self) -> Line<'a> {
        let last = self.items.len().saturating_sub(1);
        let mut spans = Vec::new();
        for (index, item) in self.items.iter().enumerate() {
            if index > 0 {
                spans.push(Span::raw(self.separator));
            }
            let style = if index == last {
                self.current_style
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

    use crate::widgets::breadcrumbs::Breadcrumbs;

    #[test]
    fn breadcrumbs_join_items() {
        let items = vec![Line::from("Home"), Line::from("Settings")];
        assert_eq!(
            Breadcrumbs::new(&items).line().to_string(),
            "Home / Settings"
        );
    }
}
