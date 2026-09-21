//! Repository-wide commit heatmap and recent contributor activity.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::theme;
use crate::components::ui::separator::Separator;
use crate::domain::profile::Activity;

const COMPACT_ACTIVITY_WIDTH: u16 = 60;

pub(super) fn lines(activity: &Activity, width: u16) -> Vec<Line<'static>> {
    let divider = |label| {
        Separator::new(label)
            .style(Style::new().fg(theme::IDLE))
            .line(width)
    };
    let mut lines = vec![divider("Repository activity · local and remote branches")];
    if width < COMPACT_ACTIVITY_WIDTH {
        lines.push(Line::from(format!(
            "{} commits in the past year",
            activity.commit_count
        )));
    } else {
        lines.extend(heatmap_lines(activity));
        lines.push(Line::from(format!(
            "{} commits in the past year",
            activity.commit_count
        )));
    }
    lines.push(divider("Contributors · past year"));
    if activity.contributors.is_empty() {
        lines.push(Line::from("No contributors in the past year"));
    } else {
        for contributor in &activity.contributors {
            let commits = contributor.commit_count;
            lines.push(Line::from(vec![
                Span::styled(
                    contributor.name.clone(),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(
                    "  {commits} commit{}",
                    if commits == 1 { "" } else { "s" }
                )),
            ]));
        }
    }
    lines.push(divider("Recent commits"));
    if activity.recent_commits.is_empty() {
        lines.push(Line::from(
            "No recent commits on local or fetched remote branches",
        ));
    } else {
        for commit in &activity.recent_commits {
            lines.push(Line::from(vec![
                Span::styled(
                    commit.author.clone(),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(commit.short_hash.clone(), Style::new().fg(theme::HASH)),
                Span::raw(format!("  {}", commit.summary)),
            ]));
        }
    }
    lines
}

fn heatmap_lines(activity: &Activity) -> Vec<Line<'static>> {
    let colors = [
        Color::Rgb(22, 27, 34),
        Color::Rgb(14, 68, 41),
        Color::Rgb(0, 109, 50),
        Color::Rgb(38, 166, 65),
        Color::Rgb(57, 211, 83),
    ];
    let labels = ["Mon", "   ", "Wed", "   ", "Fri", "   ", "   "];
    let mut lines = vec![month_line(activity)];
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
    lines
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
