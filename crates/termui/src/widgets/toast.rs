use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use crate::widgets::overlay::{Anchor, Backdrop, Overlay, OverlayState, Slide};

const DEFAULT_ANIMATION: Duration = Duration::from_millis(160);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Semantic color treatment for a toast notification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastVariant {
    #[default]
    Info,
    Success,
    Warning,
    Destructive,
}

impl ToastVariant {
    const fn style(self) -> Style {
        match self {
            Self::Info => Style::new().fg(Color::Rgb(96, 165, 250)),
            Self::Success => Style::new().fg(Color::Rgb(74, 222, 128)),
            Self::Warning => Style::new().fg(Color::Rgb(250, 204, 21)),
            Self::Destructive => Style::new()
                .fg(Color::Rgb(248, 113, 113))
                .add_modifier(Modifier::BOLD),
        }
    }
}

/// Animated, self-expiring notification rendered in the bottom-right corner.
#[must_use]
pub struct Toast {
    title: String,
    message: String,
    variant: ToastVariant,
    state: OverlayState,
    timeout: Duration,
    shown: Duration,
    width: Constraint,
    height: Constraint,
}

impl Toast {
    pub fn new<T: Into<String>, M: Into<String>>(title: T, message: M) -> Self {
        let mut state = OverlayState::new().with_duration(DEFAULT_ANIMATION);
        state.open();
        Self {
            title: title.into(),
            message: message.into(),
            variant: ToastVariant::default(),
            state,
            timeout: DEFAULT_TIMEOUT,
            shown: Duration::ZERO,
            width: Constraint::Length(48),
            height: Constraint::Length(5),
        }
    }

    pub fn variant(mut self, variant: ToastVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn animation(mut self, duration: Duration) -> Self {
        self.state = OverlayState::new().with_duration(duration);
        self.state.open();
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

    pub fn dismiss(&mut self) {
        self.state.close();
    }

    pub fn tick(&mut self, elapsed: Duration) {
        self.state.tick(elapsed);
        if self.state.is_open() {
            self.shown = self.shown.saturating_add(elapsed);
            if self.shown >= self.timeout {
                self.dismiss();
            }
        }
    }

    pub const fn is_closed(&self) -> bool {
        self.state.is_closed()
    }

    pub const fn is_closing(&self) -> bool {
        self.state.is_closing()
    }

    pub const fn is_animating(&self) -> bool {
        self.state.is_animating()
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let style = self.variant.style();
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style)
            .title(Line::styled(self.title.as_str(), style));
        let overlay = Overlay::new()
            .anchor(Anchor::BottomRight)
            .slide(Slide::Bottom)
            .width(self.width)
            .height(self.height)
            .backdrop(Backdrop::new(Color::Black).fg(Color::DarkGray))
            .bg(Color::Black)
            .block(block);
        frame.render_stateful_widget(overlay, area, &mut self.state);
        if let Some(inner) = self.state.inner_area() {
            frame.render_widget(
                Paragraph::new(self.message.as_str())
                    .wrap(Wrap { trim: true })
                    .style(style),
                inner,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::widgets::toast::{Toast, ToastVariant};

    #[test]
    fn toast_opens_and_auto_closes_after_timeout() {
        let mut toast = Toast::new("Saved", "Changes written")
            .variant(ToastVariant::Success)
            .animation(Duration::ZERO)
            .timeout(Duration::from_secs(2));
        assert!(!toast.is_closed());
        toast.tick(Duration::from_secs(2));
        assert!(toast.is_closing() || toast.is_closed());
    }

    #[test]
    fn dismiss_starts_close_transition() {
        let mut toast = Toast::new("Warning", "Check your input");
        toast.dismiss();
        assert!(toast.is_closing() || toast.is_closed());
    }
}
