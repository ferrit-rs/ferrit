//! Create-remote projection and popup rendering.

use crate::ui::components::create_remote::{Field, Step};
use ferrit_domain::host::Visibility;
use ferrit_tui::theme::palette::Palette;
use ferrit_tui::widgets::chrome::bar::KeyBar;
use ferrit_tui::widgets::chrome::dialog::Dialog;
use ferrit_tui::widgets::chrome::panel::Panel;
use ferrit_tui::widgets::text_input::TextInput;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[derive(Debug)]
pub enum CreateRemoteView<'a> {
    Checking,
    Form(FormView<'a>),
    Confirm(ConfirmView),
}

#[derive(Debug)]
pub struct FormView<'a> {
    pub name: &'a TextInput,
    pub description: &'a TextInput,
    pub visibility: Visibility,
    pub focus: Field,
    pub error: Option<&'a str>,
}

/// The last question: what will happen, and which keys answer it.
#[derive(Debug)]
pub struct ConfirmView {
    pub title: String,
    pub visibility: Visibility,
    pub lines: Vec<String>,
    pub hint: &'static str,
}

/// Consequences supplied by the repository, not by the form.
pub(crate) struct Consequences<'a> {
    pub(crate) first_commit: bool,
    pub(crate) ssh_host: Option<&'a str>,
    pub(crate) branch: &'a str,
}

impl Step {
    /// What the renderer draws of this step.
    pub(crate) fn view(&self, consequences: &Consequences<'_>) -> CreateRemoteView<'_> {
        match self {
            Self::Checking { .. } => CreateRemoteView::Checking,
            Self::Form(form) => CreateRemoteView::Form(FormView {
                name: &form.name,
                description: &form.description,
                visibility: form.visibility,
                focus: form.focus,
                error: form.error.as_deref(),
            }),
            Self::Confirm(form) => {
                let draft = form.draft();
                let word = match draft.visibility {
                    Visibility::Private => "PRIVATE",
                    Visibility::Public => "PUBLIC",
                };
                let mut lines = vec![format!("{word} repository")];
                if draft.visibility == Visibility::Public {
                    lines.push("Everyone can read its history.".to_owned());
                }
                if consequences.first_commit {
                    lines.push("first: commit an empty README.md, made by Ferrit".to_owned());
                }
                let over = consequences.ssh_host.map_or_else(String::new, |host| {
                    format!(" using your SSH key for {host}")
                });
                lines.push(format!(
                    "then: add remote `origin`{over}, push {}",
                    consequences.branch
                ));
                CreateRemoteView::Confirm(ConfirmView {
                    title: format!("Create {}", draft.target),
                    visibility: draft.visibility,
                    lines,
                    hint: match draft.visibility {
                        Visibility::Private => "Enter/y: create   n/Esc: back",
                        Visibility::Public => "y: create (Enter does not)   n/Esc: back",
                    },
                })
            },
        }
    }
}

/// Draw the `gh` check, form, or confirmation popup.
pub(crate) fn draw_create_remote(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &CreateRemoteView<'_>,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    match view {
        CreateRemoteView::Checking => {
            let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
            let dialog = Dialog::new(Line::styled(" Create on GitHub ", focused))
                .fit_content(44.min(area.width), 1, 1)
                .border_style(focused)
                .render(frame, area);
            frame.render_widget(
                Paragraph::new(Line::styled(
                    " checking gh\u{2026}",
                    Style::new().fg(palette.idle),
                )),
                dialog.body,
            );
            frame.render_widget(
                Paragraph::new(KeyBar::hints("Cancel: Esc", palette).line()),
                dialog.footer,
            );
        },
        CreateRemoteView::Form(form) => draw_create_form(frame, area, form, accent, palette),
        CreateRemoteView::Confirm(confirm) => {
            draw_create_confirm(frame, area, confirm, accent, palette);
        },
    }
}

fn draw_text_box(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    input: &TextInput,
    focused: bool,
    max: usize,
    palette_style: (Style, Style),
) {
    let style = if focused {
        palette_style.0
    } else {
        palette_style.1
    };
    let counter = Line::styled(format!(" {}/{} ", input.text().chars().count(), max), style);
    let block = Panel::new()
        .title(Line::styled(format!(" {title} "), style))
        .bottom_title(counter.right_aligned())
        .border_style(style)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if focused {
        input.render(frame, inner);
    } else {
        input.render_inactive(frame, inner);
    }
}

fn draw_create_form(
    frame: &mut Frame<'_>,
    area: Rect,
    form: &FormView<'_>,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let idle = Style::new().fg(palette.idle);
    let dialog = Dialog::new(Line::styled(" Create on GitHub ", focused))
        .fit_content(70.min(area.width), 14, 1)
        .border_style(focused)
        .render(frame, area);
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(dialog.body);
    let at = |index: usize| rows.get(index).copied().unwrap_or_default();

    draw_text_box(
        frame,
        at(0),
        "Name",
        form.name,
        form.focus == Field::Name,
        100,
        (focused, idle),
    );

    let radio = |on: bool| if on { "(\u{2022})" } else { "( )" };
    let label_style = if form.focus == Field::Visibility {
        focused
    } else {
        idle
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Visibility   ", label_style),
            Span::raw(format!(
                "{} private   {} public",
                radio(form.visibility == Visibility::Private),
                radio(form.visibility == Visibility::Public)
            )),
        ])),
        at(1),
    );

    draw_text_box(
        frame,
        at(2),
        "Description",
        form.description,
        form.focus == Field::Description,
        350,
        (focused, idle),
    );
    if let Some(error) = form.error {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {error}"),
                Style::new().fg(palette.del),
            )),
            at(3),
        );
    }
    frame.render_widget(
        Paragraph::new(KeyBar::hints("Next: Tab   Continue: Enter   Cancel: Esc", palette).line()),
        dialog.footer,
    );
}

fn draw_create_confirm(
    frame: &mut Frame<'_>,
    area: Rect,
    confirm: &ConfirmView,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let border = match confirm.visibility {
        Visibility::Private => Style::new().fg(accent).add_modifier(Modifier::BOLD),
        Visibility::Public => Style::new().fg(palette.del).add_modifier(Modifier::BOLD),
    };
    let rows = u16::try_from(confirm.lines.len())
        .unwrap_or(u16::MAX)
        .max(1);
    let hint_width = u16::try_from(confirm.hint.chars().count() + 4).unwrap_or(u16::MAX);
    let title_width = u16::try_from(confirm.title.chars().count() + 6).unwrap_or(u16::MAX);
    let text_width = confirm
        .lines
        .iter()
        .map(|line| u16::try_from(line.chars().count() + 4).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(0);
    let dialog = Dialog::new(Line::styled(format!(" {} ", confirm.title), border))
        .fit_content(
            48.max(hint_width)
                .max(title_width)
                .max(text_width)
                .min(area.width),
            rows,
            1,
        )
        .border_style(border)
        .render(frame, area);
    let lines: Vec<Line<'static>> = confirm
        .lines
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let style = if index == 0 { border } else { Style::new() };
            Line::styled(format!(" {text}"), style)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), dialog.body);
    frame.render_widget(
        Paragraph::new(Line::styled(confirm.hint, Style::new().fg(palette.idle))),
        dialog.footer,
    );
}
