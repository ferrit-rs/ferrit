//! Help entries generated from the live keymap.

use crate::ui::keymap::Keymap;
use crate::ui::keymap::context::Context;

/// One line of the help screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpLine {
    Heading(String),
    /// `keys` is left-aligned in a column, `text` beside it.
    Entry {
        keys: String,
        text: String,
    },
    Blank,
}

/// Keep headings that contain at least one matching command, so search keeps
/// the context of every result without showing unrelated commands.
pub fn filter_help_lines(lines: &[HelpLine], query: &str) -> Vec<HelpLine> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return lines.to_vec();
    }

    let mut filtered = Vec::new();
    let mut heading = None;
    let mut blank = false;
    for line in lines {
        match line {
            HelpLine::Heading(text) => heading = Some(text.clone()),
            HelpLine::Blank => blank = true,
            HelpLine::Entry { keys, text }
                if keys.to_lowercase().contains(&query) || text.to_lowercase().contains(&query) =>
            {
                if let Some(heading) = heading.take() {
                    if !filtered.is_empty() && blank {
                        filtered.push(HelpLine::Blank);
                    }
                    filtered.push(HelpLine::Heading(heading));
                }
                filtered.push(line.clone());
                blank = false;
            },
            HelpLine::Entry { .. } => {},
        }
    }
    filtered
}

/// The help screen for a focused pane: its own context, then global, then
/// keys that are not remappable. Unbound actions stay out.
pub fn help_lines(keymap: &Keymap, contexts: &[Context]) -> Vec<HelpLine> {
    let mut lines = Vec::new();
    for &context in contexts {
        let mut entries = Vec::new();
        for action in Keymap::actions_of(context) {
            let keys = keymap.keys(context, action);
            if keys.is_empty() {
                continue;
            }
            entries.push(HelpLine::Entry {
                keys: keys
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                text: action.description().to_owned(),
            });
        }
        if entries.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(HelpLine::Blank);
        }
        lines.push(HelpLine::Heading(format!("{} keys", context.title())));
        lines.extend(entries);
    }
    lines.push(HelpLine::Blank);
    lines.push(HelpLine::Heading("Fixed keys (not remappable)".to_owned()));
    for (keys, text) in FIXED_KEYS {
        lines.push(HelpLine::Entry {
            keys: (*keys).to_owned(),
            text: (*text).to_owned(),
        });
    }
    lines
}

const FIXED_KEYS: &[(&str, &str)] = &[
    ("ctrl-c", "quit, from anywhere"),
    ("y / n", "answer a confirm"),
    ("enter / esc", "in a popup: confirm / cancel"),
    ("tab", "commit popup: switch summary / description"),
    (
        "ctrl-o / ctrl-n",
        "commit popup: toggle sign-off / no-verify",
    ),
    (
        "ctrl-s / meta-enter",
        "commit popup: commit from either field",
    ),
    (
        "j / k, enter, a letter",
        "in a menu: move, run, run that row",
    ),
    (
        "wheel / click",
        "scroll the pane under it / focus a row there",
    ),
];
