//! Authors: one row per person. Commits are first grouped by (mailmap-resolved)
//! email, then the groups that share a name become one row.

use std::collections::BTreeMap;

use crate::git::stats::AuthorStat;

/// Commits of one email (or of one name, when the commit has no email).
#[derive(Default)]
pub(crate) struct AuthorAcc {
    pub(crate) name: String,
    pub(crate) email: String,
    pub(crate) commits: usize,
    pub(crate) last: i64,
}

/// Merge the per-email groups that share a name (case-insensitive, trimmed,
/// non-empty) into one `AuthorStat`, the most frequent email first (ties:
/// alphabetical). The lines are filled in later from `emails`.
pub(crate) fn merge(accs: impl IntoIterator<Item = (String, AuthorAcc)>) -> Vec<AuthorStat> {
    let mut by_name: BTreeMap<String, Vec<AuthorAcc>> = BTreeMap::new();
    for (key, acc) in accs {
        let name = acc.name.trim().to_lowercase();
        // A nameless author is not merged with the next nameless one.
        let group = if name.is_empty() {
            format!("\0{key}")
        } else {
            name
        };
        by_name.entry(group).or_default().push(acc);
    }
    by_name
        .into_values()
        .map(|mut group| {
            group.sort_by(|a, b| {
                b.commits
                    .cmp(&a.commits)
                    .then_with(|| a.email.cmp(&b.email))
            });
            let commits = group.iter().map(|a| a.commits).sum();
            let last_commit = group.iter().map(|a| a.last).max().unwrap_or(0);
            let mut emails: Vec<String> = Vec::new();
            for acc in &group {
                if !emails.contains(&acc.email) {
                    emails.push(acc.email.clone());
                }
            }
            let first = group.into_iter().next().unwrap_or_default();
            AuthorStat {
                name: first.name,
                email: first.email,
                emails,
                commits,
                added: None,
                removed: None,
                last_commit,
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "a failed lookup is the assertion in a test"
)]
mod tests {
    use crate::git::stats::authors::{AuthorAcc, merge};

    fn acc(name: &str, email: &str, commits: usize) -> (String, AuthorAcc) {
        (
            email.to_lowercase(),
            AuthorAcc {
                name: name.to_owned(),
                email: email.to_owned(),
                commits,
                last: 0,
            },
        )
    }

    #[test]
    fn case_and_surrounding_whitespace_do_not_split_a_name() {
        let merged = merge([
            acc(" Richard Lavoura ", "a@x", 1),
            acc("richard lavoura", "b@x", 2),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].commits, 3);
        assert_eq!(merged[0].emails, ["b@x", "a@x"]);
    }

    #[test]
    fn empty_names_are_never_merged() {
        let merged = merge([acc("", "a@x", 1), acc("  ", "b@x", 1)]);
        assert_eq!(merged.len(), 2);
    }
}
