//! The command log: the strip under the panes and the full view.

use crate::git::command_log;
use crate::theme::palette::Palette;
use crate::tui::components::diff::views::CommandLogView;
use crate::tui::draw::Landed;
use crate::tui::scene::Scene;
use crate::tui::widgets::chrome::Dialog;
use crate::tui::widgets::chrome::Panel;
use crate::tui::{mock, row_lines};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

/// The `@` viewer: every recorded command, newest at the bottom, scrolled up
/// by `view.from_bottom` rows (clamped to what exists).
pub(crate) fn draw_command_log_view(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &CommandLogView,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(" command log ", focused))
        .size(
            (area.width * 4 / 5).max(40.min(area.width)),
            (area.height * 4 / 5).max(8.min(area.height)),
        )
        .footer_rows(1)
        .border_style(focused)
        .render(frame, area);
    let rows = usize::from(dialog.body.height);
    // Every record's command and its answer lines, one row each.
    let all: Vec<Line<'static>> = view
        .records
        .iter()
        .flat_map(|record| row_lines::command_lines(palette, record))
        .collect();
    let total = all.len();
    let end = total.saturating_sub(view.from_bottom.min(total.saturating_sub(rows)));
    let start = end.saturating_sub(rows);
    let lines: Vec<Line<'static>> = all.get(start..end).unwrap_or_default().to_vec();
    let body = if lines.is_empty() {
        vec![Line::styled(
            "no git command run yet",
            Style::new().fg(palette.idle),
        )]
    } else {
        lines
    };
    frame.render_widget(Paragraph::new(body), dialog.body);
    frame.render_widget(
        Paragraph::new(Line::styled(
            format!("{end}/{total} \u{b7} j/k scroll \u{b7} g/G oldest/newest \u{b7} Esc close"),
            Style::new().fg(palette.idle),
        )),
        dialog.footer,
    );
}

/// Files pane with a real diff selected: lazygit's own two-column split,
/// Unstaged Changes beside Staged Changes, in place of the single right
/// pane every other selection uses (`draw_right_pane`). Deliberately
/// simplified against that path: each side renders directly through
/// `row_lines::render_diff` / `render_delta`, bypassing `App::rendered_diff`'s
/// cache (it is keyed for one diff at a time) and skipping the `]` / `[`
/// hunk-focus highlight — the two columns just scroll together on the one
/// `app.right_scroll()`.
pub(crate) fn draw_command_log(
    frame: &mut Frame<'_>,
    app: &Scene<'_>,
    landed: &mut Landed,
    area: Rect,
) {
    let palette = &app.palette();
    let git_user_name = app.git_user_name().map(str::to_owned);
    let [heading, panel_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    frame.render_widget(
        Paragraph::new("Infos").style(Style::new().fg(palette.idle)),
        heading,
    );

    let block = Panel::new()
        .border_style(Style::new().fg(palette.idle))
        .block();
    let inner = block.inner(panel_area);
    frame.render_widget(block, panel_area);

    let [first, rest, dashboard_row] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    landed.author = Some(Rect::ZERO);
    landed.dashboard = Some(Rect::ZERO);

    // The two newest commands ferrit ran (writes only; `@` lists everything),
    // oldest on top. A repo-free `App::mock()` keeps its fixed sample.
    let rows = usize::from(rest.height);
    let lines: Vec<Line<'static>> = if app.is_mock() {
        mock::COMMAND_LOG
            .iter()
            .map(|command| row_lines::log_line(palette, command))
            .collect()
    } else {
        let mut lines = command_log_lines(app);
        let extra = lines.len().saturating_sub(rows);
        lines.drain(..extra);
        lines
    };
    let first_line = lines.first().cloned().unwrap_or_default();
    if let Some(name) = git_user_name {
        let author = format!("👤 {name}");
        let author_width =
            u16::try_from(UnicodeWidthStr::width(author.as_str())).unwrap_or(u16::MAX);
        let [command_area, author_area] = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(author_width.min(first.width)),
        ])
        .areas(first);
        frame.render_widget(Paragraph::new(first_line), command_area);
        frame.render_widget(
            Paragraph::new(author)
                .alignment(Alignment::Right)
                .style(Style::new().fg(palette.idle)),
            author_area,
        );
        landed.author = Some(author_area);
    } else {
        frame.render_widget(Paragraph::new(first_line), first);
    }
    if lines.len() > 1 {
        frame.render_widget(
            Paragraph::new(lines.get(1..).unwrap_or_default().to_vec()),
            rest,
        );
    }
    let trigger = "📊 Dashboard";
    let trigger_width = u16::try_from(UnicodeWidthStr::width(trigger)).unwrap_or(u16::MAX);
    let [_, dashboard_area] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(trigger_width.min(dashboard_row.width)),
    ])
    .areas(dashboard_row);
    frame.render_widget(
        Paragraph::new(trigger)
            .alignment(Alignment::Right)
            .style(Style::new().fg(palette.idle)),
        dashboard_area,
    );
    landed.dashboard = Some(dashboard_area);
}

/// Inner rows of the Infos box: the two it always has, grown to hold the newest
/// command and every line git answered with (`MAX_OUTPUT_LINES` at most), but never
/// more than a third of the screen so the panes above keep their room.
pub(crate) fn command_log_rows(app: &Scene<'_>, screen_height: u16) -> u16 {
    if app.is_mock() {
        return 2;
    }
    let newest = command_log::recent(1, app.prefs.config.log.show_reads)
        .last()
        .map_or(0, |record| {
            row_lines::command_lines(&app.palette(), record).len()
        });
    let wanted = u16::try_from(newest).unwrap_or(u16::MAX).max(2);
    wanted.min((screen_height / 3).saturating_sub(3).max(2))
}

/// The two newest commands' lines, each command followed by git's answer.
fn command_log_lines(app: &Scene<'_>) -> Vec<Line<'static>> {
    command_log::recent(2, app.prefs.config.log.show_reads)
        .iter()
        .flat_map(|record| row_lines::command_lines(&app.palette(), record))
        .collect()
}
