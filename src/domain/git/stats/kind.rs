//! The kind of change a commit is, read from the Conventional Commits prefix
//! of its subject (`feat:`, `fix(scope):`, `feat!:`). No git here.

/// Kinds the dashboard tells apart; anything else is `Other`. The variant
/// order is the tie-break when two kinds have the same count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Feat,
    Fix,
    Docs,
    Test,
    Refactor,
    Perf,
    Style,
    Build,
    Ci,
    Chore,
    Other,
}

impl Kind {
    pub const ALL: [Self; 11] = [
        Self::Feat,
        Self::Fix,
        Self::Docs,
        Self::Test,
        Self::Refactor,
        Self::Perf,
        Self::Style,
        Self::Build,
        Self::Ci,
        Self::Chore,
        Self::Other,
    ];

    /// The prefix word, `other` for the rest.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Feat => "feat",
            Self::Fix => "fix",
            Self::Docs => "docs",
            Self::Test => "test",
            Self::Refactor => "refactor",
            Self::Perf => "perf",
            Self::Style => "style",
            Self::Build => "build",
            Self::Ci => "ci",
            Self::Chore => "chore",
            Self::Other => "other",
        }
    }
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
    Kind::ALL
        .into_iter()
        .filter(|kind| *kind != Kind::Other)
        .find(|kind| word.eq_ignore_ascii_case(kind.name()))
        .unwrap_or(Kind::Other)
}

#[cfg(test)]
mod tests {
    use super::{Kind, parse_kind};

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
