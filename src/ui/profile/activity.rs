//! Terminal rendering for the profile contribution heatmap.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::domain::profile::Activity;
use crate::theme;

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, activity: &Activity) {
    if area.width < 82 {
        frame.render_widget(
            Paragraph::new("Widen terminal to view activity chart"),
            area,
        );
        return;
    }
    let colors = [
        Color::Rgb(22, 27, 34),
        Color::Rgb(14, 68, 41),
        Color::Rgb(0, 109, 50),
        Color::Rgb(38, 166, 65),
        Color::Rgb(57, 211, 83),
    ];
    let mut lines = vec![month_line(activity)];
    let labels = ["Mon", "   ", "Wed", "   ", "Fri", "   ", "   "];
    for row in 0..7 {
        let mut spans = vec![
            Span::styled(
                labels.get(row).copied().unwrap_or("   "),
                Style::new().fg(theme::IDLE),
            ),
            Span::raw(" "),
        ];
        for week in &activity.weeks {
            let count = week.days.get(row).copied().unwrap_or(0);
            let level = match count {
                0 => 0,
                1 => 1,
                2..=3 => 2,
                4..=7 => 3,
                _ => 4,
            };
            spans.push(Span::styled(
                "■",
                Style::new().fg(colors.get(level).copied().unwrap_or(Color::Green)),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        "{} commits · {} Ferrit pushes in past year",
        activity.commit_count, activity.push_count
    )));
    lines.push(Line::styled(
        "Squares = commits/day · pushes via Ferrit only",
        Style::new().fg(theme::IDLE),
    ));
    frame.render_widget(Paragraph::new(lines), area);
}

fn month_line(activity: &Activity) -> Line<'static> {
    let mut spans = vec![Span::raw("    ")];
    for week in &activity.weeks {
        if let Some(month) = week.month_label {
            spans.push(Span::styled(
                format!("{month:<3}"),
                Style::new().fg(theme::IDLE),
            ));
        } else {
            spans.push(Span::raw(" "));
        }
    }
    Line::from(spans)
}
