//! Turns a script into a `vhs` tape, for the human-facing screenshots of
//! `docs/PLAN_SELF_TESTING.md`. Never a gate: it only maps the directives a
//! real terminal session can reproduce (keys, typing, snapshots) and says which
//! script it skipped and why when it cannot (`exec`, `write`, `config`,
//! `async-key` change the world from outside the terminal).

use ratatui::crossterm::event::KeyCode;

use super::script::{Directive, Script};
use crate::app::keymap::KeyBinding;

/// A pause between keys so the GIF can be followed.
const PACE: &str = "Sleep 250ms";

/// The tape for `script`, named `name`, or why it cannot have one.
pub fn tape(name: &str, script: &Script) -> Result<String, String> {
    let mut fixture = None;
    let mut body = Vec::new();
    for step in &script.steps {
        match &step.directive {
            Directive::Fixture(f) => fixture = Some(f.clone()),
            Directive::Size(..) | Directive::Resize(..) => {},
            Directive::Key(keys) => {
                for &key in keys {
                    body.push(vhs_key(key));
                    body.push(PACE.to_owned());
                }
            },
            Directive::Type(text) => {
                body.push(format!("Type \"{}\"", text.replace('"', "\\\"")));
                body.push(PACE.to_owned());
            },
            Directive::Snapshot(label) => {
                body.push(format!("Screenshot test/shots/{name}-{label}.png"));
            },
            Directive::Refresh
            | Directive::ExpectText(_)
            | Directive::ExpectNoText(_)
            | Directive::Git { .. } => {},
            Directive::Exec(_)
            | Directive::Write { .. }
            | Directive::Config(_)
            | Directive::AsyncKey(_) => {
                return Err(format!(
                    "line {}: it changes the repository or the configuration from outside the terminal, which a tape cannot do",
                    step.line
                ));
            },
        }
    }
    let fixture = fixture.ok_or_else(|| "no `fixture` directive".to_owned())?;
    let mut out = vec![
        format!("Output test/shots/{name}.gif"),
        "Set Shell \"bash\"".to_owned(),
        "Set FontSize 14".to_owned(),
        "Set Width 1400".to_owned(),
        "Set Height 900".to_owned(),
        "Hide".to_owned(),
        format!("Type \"cd $(ferrit --fixture {fixture}) && clear\""),
        "Enter".to_owned(),
        "Show".to_owned(),
        "Type \"ferrit\"".to_owned(),
        "Enter".to_owned(),
        "Sleep 1s".to_owned(),
    ];
    out.extend(body);
    let mut text = out.join("\n");
    text.push('\n');
    Ok(text)
}

/// One key as a `vhs` command.
fn vhs_key(key: KeyBinding) -> String {
    let name = match key.code {
        KeyCode::Enter => "Enter".to_owned(),
        KeyCode::Esc => "Escape".to_owned(),
        KeyCode::Char(' ') => "Space".to_owned(),
        KeyCode::Tab => "Tab".to_owned(),
        KeyCode::BackTab => return "Shift+Tab".to_owned(),
        KeyCode::Up => "Up".to_owned(),
        KeyCode::Down => "Down".to_owned(),
        KeyCode::Left => "Left".to_owned(),
        KeyCode::Right => "Right".to_owned(),
        KeyCode::PageUp => "PageUp".to_owned(),
        KeyCode::PageDown => "PageDown".to_owned(),
        KeyCode::Home => "Home".to_owned(),
        KeyCode::End => "End".to_owned(),
        KeyCode::Backspace => "Backspace".to_owned(),
        KeyCode::Char(c) if !key.ctrl && !key.alt => {
            return format!(
                "Type \"{}\"",
                if c == '"' {
                    "\\\"".to_owned()
                } else {
                    c.to_string()
                }
            );
        },
        KeyCode::Char(c) => c.to_string(),
        other => format!("{other:?}"),
    };
    let mut prefix = String::new();
    if key.ctrl {
        prefix.push_str("Ctrl+");
    }
    if key.alt {
        prefix.push_str("Alt+");
    }
    format!("{prefix}{name}")
}
