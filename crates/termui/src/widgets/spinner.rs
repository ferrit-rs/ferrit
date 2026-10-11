use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const DEFAULT_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Stateless spinner frame. The parent advances the frame index on each tick.
#[must_use]
pub struct Spinner<'a> {
    label: &'a str,
    frame: usize,
    frames: &'static [char],
    style: Style,
}

impl<'a> Spinner<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            frame: 0,
            frames: DEFAULT_FRAMES,
            style: Style::default().add_modifier(Modifier::DIM),
        }
    }

    pub fn frame(mut self, frame: usize) -> Self {
        self.frame = frame;
        self
    }

    pub fn frames(mut self, frames: &'static [char]) -> Self {
        self.frames = frames;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn line(&self) -> Line<'static> {
        let glyph = self
            .frames
            .get(self.frame % self.frames.len().max(1))
            .copied()
            .unwrap_or(' ');
        Line::from(vec![
            Span::styled(glyph.to_string(), self.style),
            Span::raw(format!(" {}", self.label)),
        ])
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Paragraph::new(self.line()), area);
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::spinner::Spinner;

    #[test]
    fn spinner_wraps_frame_index() {
        assert_eq!(
            Spinner::new("Loading").frame(10).line().to_string(),
            "⠋ Loading"
        );
    }
}
