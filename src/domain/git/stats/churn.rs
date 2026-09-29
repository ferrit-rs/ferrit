//! Lines added and removed, per author and per file, from one `git log
//! --numstat` through `exec` (so the command log shows it). `git2` diffing
//! every commit is much slower on a big history. Failure gives `None` and the
//! rest of the statistics still fill.

use std::collections::BTreeMap;

use git2::Repository;

use super::{FileStat, HotFiles, Lines};
use crate::domain::git::exec;
use crate::domain::git::stats::share::Share;

/// Files listed as hot.
const HOT_FILES: usize = 10;

/// Lockfiles: their change count says nothing about the code.
const LOCKFILES: [&str; 8] = [
    "cargo.lock",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "poetry.lock",
    "go.sum",
    "gemfile.lock",
    "composer.lock",
];

/// Changelog-like files: the name, or the name then `.` or `-` and anything
/// (`CHANGELOG.md`, `NEWS-1.2`), not a bare prefix that would hide `news_feed.rs`.
const CHANGELOGS: [&str; 4] = ["changelog", "history", "news", "releases"];

#[derive(Default)]
struct FileAcc {
    commits: usize,
    added: u64,
    removed: u64,
}

/// What one `git log --numstat` said, before it becomes `HotFiles`.
#[derive(Default)]
pub(super) struct Churn {
    /// Non-merge commits read.
    pub(super) commits: usize,
    /// The cap was hit: older commits exist that were not read.
    pub(super) sampled: bool,
    pub(super) lines: Lines,
    /// Lines by lower-cased mailmap-resolved author email.
    pub(super) authors: BTreeMap<String, Lines>,
    files: BTreeMap<String, FileAcc>,
}

/// Is `path` a file whose change count is noise (lockfile, changelog)?
fn is_ignored(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    LOCKFILES.contains(&name.as_str())
        || CHANGELOGS.iter().any(|stem| {
            name.strip_prefix(stem)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '-']))
        })
}

/// Read the churn of the non-merge commits of the local and remote branches
/// (and a detached `HEAD`) since `since`, at most `cap` of them, newest first.
pub(super) fn read(repo: &Repository, since: Option<i64>, cap: usize) -> Option<Churn> {
    let workdir = repo.workdir().unwrap_or_else(|| repo.path());
    let mut cmd = exec::git(workdir);
    // Print paths as they are, not C-quoted, without changing the logged argv.
    cmd.env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.quotepath")
        .env("GIT_CONFIG_VALUE_0", "false");
    cmd.args(["log", "--branches", "--remotes"]);
    if repo.head_detached().unwrap_or(false) {
        cmd.arg("HEAD");
    }
    cmd.args([
        "--no-merges",
        "--numstat",
        "--no-renames",
        "--format=@%H%x09%aE",
    ]);
    // One more than the cap tells a history of exactly `cap` from a longer one.
    cmd.arg(format!("-n{}", cap.saturating_add(1)));
    if let Some(since) = since {
        cmd.arg(format!("--since={since}"));
    }
    let out = exec::output(&mut cmd).ok()?;
    out.status
        .success()
        .then(|| parse(&String::from_utf8_lossy(&out.stdout), cap))
}

/// Parse `@<hash>\t<email>` headers each followed by `added\tremoved\tpath`
/// lines (`-` for a binary file), stopping at the `cap`-th commit.
fn parse(text: &str, cap: usize) -> Churn {
    let mut churn = Churn::default();
    let mut author = String::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix('@') {
            if churn.commits >= cap {
                churn.sampled = true;
                break;
            }
            churn.commits += 1;
            author = header
                .split_once('\t')
                .map_or("", |(_, email)| email)
                .to_lowercase();
            churn.authors.entry(author.clone()).or_default();
            continue;
        }
        let mut fields = line.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (added, removed) = (added.parse().unwrap_or(0), removed.parse().unwrap_or(0));
        churn.lines.added += added;
        churn.lines.removed += removed;
        if let Some(by_author) = churn.authors.get_mut(&author) {
            by_author.added += added;
            by_author.removed += removed;
        }
        let file = churn.files.entry(path.to_owned()).or_default();
        file.commits += 1;
        file.added += added;
        file.removed += removed;
    }
    churn
}

impl Churn {
    /// The 10 files most commits touched, the ignored ones counted apart.
    pub(super) fn hot_files(&self) -> HotFiles {
        let mut kept = Vec::new();
        let mut hidden = Vec::new();
        for (path, file) in &self.files {
            if is_ignored(path) {
                hidden.push((path.clone(), file.commits));
            } else {
                kept.push((path, file));
            }
        }
        kept.sort_by(|a, b| b.1.commits.cmp(&a.1.commits).then_with(|| a.0.cmp(b.0)));
        hidden.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let whole = self.commits as u64;
        HotFiles {
            files: kept
                .into_iter()
                .take(HOT_FILES)
                .map(|(path, file)| FileStat {
                    path: path.clone(),
                    share: Share::of(file.commits as u64, whole),
                    added: file.added,
                    removed: file.removed,
                })
                .collect(),
            hidden: hidden.into_iter().map(|(path, _)| path).collect(),
            commits: self.commits,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "a failed lookup is the assertion in a test"
)]
mod tests {
    use super::{is_ignored, parse};

    #[test]
    fn lockfiles_and_changelogs_are_ignored_at_any_depth() {
        for path in [
            "Cargo.lock",
            "web/package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
            "poetry.lock",
            "go.sum",
            "Gemfile.lock",
            "composer.lock",
            "CHANGELOG.md",
            "docs/changelog.txt",
            "HISTORY",
            "NEWS.rst",
            "RELEASES.md",
        ] {
            assert!(is_ignored(path), "{path}");
        }
        for path in [
            "src/app.rs",
            "Cargo.toml",
            "src/news_feed.rs",
            "lock.rs",
            "history_view.rs",
        ] {
            assert!(!is_ignored(path), "{path}");
        }
    }

    #[test]
    fn a_log_is_parsed_into_lines_authors_and_files() {
        let text = "@aaa\tMax@Example.com\n\n3\t1\tsrc/a.rs\n-\t-\timg.png\n\n@bbb\tri@x.com\n\n5\t0\tsrc/a.rs\n";
        let churn = parse(text, 10);
        assert_eq!(churn.commits, 2);
        assert!(!churn.sampled);
        assert_eq!((churn.lines.added, churn.lines.removed), (8, 1));
        assert_eq!(churn.authors["max@example.com"].added, 3);
        assert_eq!(churn.authors["ri@x.com"].added, 5);
        let hot = churn.hot_files();
        assert_eq!(hot.files[0].path, "src/a.rs");
        assert_eq!(hot.files[0].share.count, 2);
        assert_eq!(hot.files[0].share.percent, Some(100));
        assert_eq!((hot.files[0].added, hot.files[0].removed), (8, 1));
        assert_eq!(hot.files[1].path, "img.png");
    }

    #[test]
    fn the_cap_stops_at_the_commit_after_it_and_says_sampled() {
        let text = "@a\tx@x\n1\t0\tf\n@b\tx@x\n1\t0\tf\n@c\tx@x\n1\t0\tf\n";
        let churn = parse(text, 2);
        assert_eq!(churn.commits, 2);
        assert!(churn.sampled);
        assert_eq!(churn.lines.added, 2);
        assert!(!parse(text, 3).sampled);
    }
}
