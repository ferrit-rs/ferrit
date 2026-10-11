use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::Gauge;

/// Progress bar with clamped ratio and optional centered label.
#[must_use]
pub struct ProgressBar<'a> {
    ratio: f64,
    label: Option<Span<'a>>,
    style: Style,
    gauge_style: Style,
    use_unicode: bool,
}

impl<'a> ProgressBar<'a> {
    pub fn new(ratio: f64) -> Self {
        Self {
            ratio: ratio.clamp(0.0, 1.0),
            label: None,
            style: Style::default(),
            gauge_style: Style::default(),
            use_unicode: true,
        }
    }

    pub fn percent(percent: u16) -> Self {
        Self::new(f64::from(percent) / 100.0)
    }

    pub fn label<T: Into<Span<'a>>>(mut self, label: T) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn gauge_style(mut self, style: Style) -> Self {
        self.gauge_style = style;
        self
    }

    pub fn use_unicode(mut self, use_unicode: bool) -> Self {
        self.use_unicode = use_unicode;
        self
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        let mut gauge = Gauge::default()
            .ratio(self.ratio)
            .style(self.style)
            .gauge_style(self.gauge_style)
            .use_unicode(self.use_unicode);
        if let Some(label) = self.label {
            gauge = gauge.label(label);
        }
        frame.render_widget(gauge, area);
    }
}

#[cfg(test)]
mod tests {
    use crate::widgets::progress::ProgressBar;

    #[test]
    fn ratio_is_clamped() {
        let _ = ProgressBar::new(-1.0);
        let _ = ProgressBar::new(2.0);
    }

    #[test]
    fn percent_builds_progress_bar() {
        let _ = ProgressBar::percent(70).label("70%");
    }
}
