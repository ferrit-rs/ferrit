//! The git config screen: `screen`.

use crate::git::config::{ConfigEntry, ConfigView, Origin, Scope, WriteScope, display_value};
use crate::git::config_edit::PickTarget;

/// One listed value and what the listing says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRow {
    pub entry: ConfigEntry,
    /// Read from a file the scope's own file includes.
    pub included: bool,
    /// A higher scope sets the same key, so git does not use this value.
    pub shadowed: bool,
    /// The key is set at several scopes and this is the value git uses.
    pub winner: bool,
}

/// A row's identity across a re-read: key, scope and value.
pub(crate) type RowId = (String, Scope, String);

#[derive(Debug)]
pub struct GitConfigScreen {
    pub(crate) view: ConfigView,
    /// The listing after the filter, ordered by key (git's order within a key).
    pub rows: Vec<ConfigRow>,
    pub filter: String,
    /// Typing goes to the filter.
    pub filtering: bool,
    pub selected: usize,
    /// The first list item of the last frame, kept so the view scrolls only
    /// when the selection leaves it.
    pub(crate) offset: usize,
    /// The file a write lands in: local or global, `s` flips it.
    pub scope: WriteScope,
    /// What the last action did, for the footer.
    pub note: Option<String>,
    /// The allowed-values menu that is up, and what choosing a row sets.
    pub(crate) pick: Option<PickTarget>,
    /// The first write to the global file was confirmed: it is asked once a session.
    pub(crate) global_confirmed: bool,
}

impl Default for GitConfigScreen {
    fn default() -> Self {
        Self {
            view: ConfigView::default(),
            rows: Vec::new(),
            filter: String::new(),
            filtering: false,
            selected: 0,
            offset: 0,
            scope: WriteScope::Local,
            note: None,
            pick: None,
            global_confirmed: false,
        }
    }
}

impl GitConfigScreen {
    /// Every value git knows, filter or not.
    pub fn total(&self) -> usize {
        self.view.entries.len()
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// The value git uses for `key`.
    pub fn effective(&self, key: &str) -> Option<&ConfigEntry> {
        self.view.effective(key)
    }

    /// The file the global scope reads and writes, as the listing shows it.
    pub(crate) fn global_file(&self) -> Option<&std::path::Path> {
        self.view
            .entries
            .iter()
            .find(|e| e.scope == Scope::Global)
            .and_then(|e| match &e.origin {
                Origin::File(path) => Some(path.as_path()),
                Origin::CommandLine | Origin::Other(_) => None,
            })
    }

    pub fn selected_row(&self) -> Option<&ConfigRow> {
        self.rows.get(self.selected)
    }

    fn selected_id(&self) -> Option<RowId> {
        self.selected_row()
            .map(|r| (r.entry.key.clone(), r.entry.scope, r.entry.value.clone()))
    }

    /// Take a fresh listing and rebuild the rows, keeping the selection on the
    /// same value, else on the same key, else near where it was.
    pub(crate) fn set_view(&mut self, view: ConfigView) {
        let before = self.selected_id();
        self.view = view;
        self.rebuild(before, false);
    }

    pub(crate) fn set_filter(&mut self, filter: String) {
        let before = self.selected_id();
        self.filter = filter;
        self.rebuild(before, true);
    }

    /// `top`: when the selected value is gone from the rows, go to the first
    /// one (a new filter) instead of staying near where it was (a re-read).
    fn rebuild(&mut self, before: Option<RowId>, top: bool) {
        let needle = self.filter.to_lowercase();
        let mut order: Vec<&ConfigEntry> = self.view.entries.iter().collect();
        order.sort_by(|a, b| a.key.cmp(&b.key));
        self.rows = order
            .into_iter()
            .filter(|e| {
                needle.is_empty()
                    || e.key.to_lowercase().contains(&needle)
                    || display_value(&e.key, &e.value)
                        .to_lowercase()
                        .contains(&needle)
            })
            .map(|entry| {
                let same_key = self.view.entries.iter().filter(|e| e.key == entry.key);
                let shadowed = same_key.clone().any(|e| e.scope > entry.scope);
                let several_scopes = same_key.clone().any(|e| e.scope != entry.scope);
                ConfigRow {
                    entry: entry.clone(),
                    included: self.view.is_included(entry),
                    shadowed,
                    winner: several_scopes && !shadowed,
                }
            })
            .collect();
        let found = before.and_then(|(key, scope, value)| {
            self.rows
                .iter()
                .position(|r| {
                    r.entry.key == key && r.entry.scope == scope && r.entry.value == value
                })
                .or_else(|| self.rows.iter().position(|r| r.entry.key == key))
        });
        self.selected = found.unwrap_or_else(|| {
            if top {
                0
            } else {
                self.selected.min(self.rows.len().saturating_sub(1))
            }
        });
    }

    pub(crate) fn move_by(&mut self, rows: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(rows).min(last);
    }
}
