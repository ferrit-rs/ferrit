use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use crate::widgets::overlay::{Anchor, Backdrop, Easing, Overlay, OverlayState, Slide};

/// Modal or sheet shell backed by the generic overlay state machine.
#[must_use]
pub struct Dialog {
    title: String,
    message: String,
    footer: Option<String>,
    style: Style,
    width: Constraint,
    height: Constraint,
    anchor: Anchor,
    slide: Option<Slide>,
    backdrop: Option<Backdrop>,
    background: Option<Color>,
    duration: Duration,
    easing: Easing,
}

impl Dialog {
    pub fn new<T: Into<String>, M: Into<String>>(title: T, message: M) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            footer: None,
            style: Style::default().fg(Color::Rgb(228, 228, 231)),
            width: Constraint::Percentage(70),
            height: Constraint::Length(9),
            anchor: Anchor::Center,
            slide: Some(Slide::Bottom),
            backdrop: Some(Backdrop::new(Color::Black).fg(Color::DarkGray)),
            background: Some(Color::Black),
            duration: Duration::from_millis(160),
            easing: Easing::EaseOut,
        }
    }

    pub fn footer<F: Into<String>>(mut self, footer: F) -> Self {
        self.footer = Some(footer.into());
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn width(mut self, width: Constraint) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Constraint) -> Self {
        self.height = height;
        self
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn slide(mut self, slide: Slide) -> Self {
        self.slide = Some(slide);
        self
    }

    pub fn no_slide(mut self) -> Self {
        self.slide = None;
        self
    }

    /// Configure a full-height side sheet.
    pub fn sheet(mut self, side: Slide) -> Self {
        self.anchor = match side {
            Slide::Left => Anchor::Left,
            Slide::Right => Anchor::Right,
            Slide::Top => Anchor::Top,
            Slide::Bottom => Anchor::Bottom,
        };
        self.width = match side {
            Slide::Left | Slide::Right => Constraint::Percentage(40),
            Slide::Top | Slide::Bottom => Constraint::Percentage(100),
        };
        self.height = match side {
            Slide::Left | Slide::Right => Constraint::Percentage(100),
            Slide::Top | Slide::Bottom => Constraint::Percentage(40),
        };
        self.slide = Some(side);
        self
    }

    pub fn backdrop(mut self, backdrop: Backdrop) -> Self {
        self.backdrop = Some(backdrop);
        self
    }

    pub fn no_backdrop(mut self) -> Self {
        self.backdrop = None;
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    pub fn transparent(mut self) -> Self {
        self.background = None;
        self
    }

    pub fn animation(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    pub fn state(&self) -> OverlayState {
        OverlayState::new()
            .with_duration(self.duration)
            .with_easing(self.easing)
    }

    pub fn render(&self, frame: &mut Frame<'_>, area: Rect, state: &mut OverlayState) {
        let title_style = self.style.add_modifier(Modifier::BOLD);
        let mut block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.style)
            .title(Line::styled(self.title.as_str(), title_style));
        if let Some(footer) = self.footer.as_deref() {
            block = block.title_bottom(Line::from(Span::styled(footer, self.style)));
        }

        let mut overlay = Overlay::new()
            .anchor(self.anchor)
            .width(self.width)
            .height(self.height)
            .backdrop_option(self.backdrop)
            .block(block);
        if let Some(slide) = self.slide {
            overlay = overlay.slide(slide);
        }
        if let Some(background) = self.background {
            overlay = overlay.bg(background);
        }
        frame.render_stateful_widget(overlay, area, state);

        if let Some(inner) = state.inner_area() {
            frame.render_widget(
                Paragraph::new(self.message.as_str())
                    .style(self.style)
                    .wrap(Wrap { trim: true }),
                inner,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Constraint;

    use crate::widgets::dialog::Dialog;
    use crate::widgets::overlay::{Anchor, Slide};

    #[test]
    fn dialog_creates_animated_overlay_state() {
        let dialog = Dialog::new("Confirm", "Continue?").footer("Enter confirm | Esc cancel");
        assert!(dialog.state().is_closed());
    }

    #[test]
    fn sheet_uses_side_anchor_and_slide() {
        let _ = Dialog::new("Details", "Body")
            .sheet(Slide::Right)
            .width(Constraint::Length(32))
            .anchor(Anchor::Right);
    }
}
