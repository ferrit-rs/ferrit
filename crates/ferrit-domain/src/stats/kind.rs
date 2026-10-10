//! The kind of change a commit is, read from the Conventional Commits prefix
//! of its subject (`feat:`, `fix(scope):`, `feat!:`). No git here.

use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};

/// Kinds the dashboard tells apart; anything else is `Other`. The variant
/// order is the tie-break when two kinds have the same count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum Kind {
    /// A new feature.
    Feat,
    /// A bug fix.
    Fix,
    /// Documentation only.
    Docs,
    /// Tests only.
    Test,
    /// A change that is neither a feature nor a fix.
    Refactor,
    /// A performance improvement.
    Perf,
    /// Formatting and style, no change of meaning.
    Style,
    /// The build system or dependencies.
    Build,
    /// Continuous integration.
    Ci,
    /// Maintenance that touches neither source nor tests.
    Chore,
    /// No recognised prefix.
    Other,
}

/// `type[(scope)][!]: description`, the type case-insensitive. A subject that
/// does not have that shape, or has another type (`revert:`, `wip:`), is
/// `Other`, and so is `Revert "..."` and `Merge ...`.
pub fn parse_kind(subject: &str) -> Kind {
    let Some((head, rest)) = subject.split_once(':') else {
        return Kind::Other;
    };
    if !rest.starts_with(char::is_whitespace) {
        return Kind::Other;
    }
    let head = head.strip_suffix('!').unwrap_or(head);
    let word = match head.split_once('(') {
        Some((word, scope)) if scope.ends_with(')') && scope.len() > 1 => word,
        Some(_) => return Kind::Other,
        None => head,
    };
    Kind::iter()
        .filter(|kind| *kind != Kind::Other)
        .find(|kind| {
            let name: &'static str = (*kind).into();
            word.eq_ignore_ascii_case(name)
        })
        .unwrap_or(Kind::Other)
}

#[cfg(test)]
mod tests {
    use crate::stats::kind::{Kind, parse_kind};

    #[test]
    fn plain_scoped_and_breaking_prefixes() {
        assert_eq!(parse_kind("feat: add a thing"), Kind::Feat);
        assert_eq!(parse_kind("fix(scope): a bug"), Kind::Fix);
        assert_eq!(parse_kind("feat!: drop the old api"), Kind::Feat);
        assert_eq!(parse_kind("refactor(app)!: move it"), Kind::Refactor);
        assert_eq!(parse_kind("docs(plan): phase 13"), Kind::Docs);
        assert_eq!(parse_kind("chore(dev): a flow"), Kind::Chore);
        assert_eq!(parse_kind("ci: x"), Kind::Ci);
    }

    #[test]
    fn the_type_is_case_insensitive() {
        assert_eq!(parse_kind("Fix: typo"), Kind::Fix);
    }

    #[test]
    fn everything_else_is_other() {
        for subject in [
            "Revert \"feat: add a thing\"",
            "Merge branch 'x' into main",
            "Merge pull request #4 from a/b",
            "revert: feat: add a thing",
            "wip: half done",
            "feat:no space",
            "feat add a thing",
            "feat(: broken scope",
            "feat(): empty scope",
            "fix (scope): space before scope",
            "",
            ":",
            "Update readme",
        ] {
            assert_eq!(parse_kind(subject), Kind::Other, "{subject:?}");
        }
    }
}
