//! Hand-built data and render helpers shared by the dashboard screen tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use ferrit::git::repo::Repo;
use ferrit::git::stats::{RepoStats, StatsOptions, Window};

pub(crate) const NOW: i64 = 1_790_683_200;
pub(crate) const DAY: i64 = 86_400;

pub(crate) fn ago(days: i64) -> i64 {
    NOW - days * DAY
}

pub(crate) static NEXT_DIR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        // The clock ticks in microseconds on macOS: tests starting together need a counter too.
        let n = NEXT_DIR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        path.push(format!("ferrit-{tag}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    git_as(dir, ("Fixture", "fixture@example.com", NOW), args)
}

pub(crate) fn git_as(dir: &Path, who: (&str, &str, i64), args: &[&str]) -> String {
    let date = format!("{} +0000", who.2);
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", who.0)
        .env("GIT_AUTHOR_EMAIL", who.1)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_NAME", who.0)
        .env("GIT_COMMITTER_EMAIL", who.1)
        .env("GIT_COMMITTER_DATE", &date)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

pub(crate) fn init(dir: &Path, branch: &str) {
    git(dir, &["init", "-q", "-b", branch]);
}

/// Write `files`, stage everything and commit as `who` at `when`; returns the hash.
pub(crate) fn commit(
    dir: &Path,
    who: (&str, &str),
    when: i64,
    message: &str,
    files: &[(&str, &str)],
) -> String {
    for (name, content) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let who = (who.0, who.1, when);
    git_as(dir, who, &["add", "-A"]);
    git_as(dir, who, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

pub(crate) const RICHARD: (&str, &str) = ("Richard", "richard@example.com");
pub(crate) const MAX_OLD: (&str, &str) = ("Max", "max@old.example.com");
pub(crate) const MAX_NEW: (&str, &str) = ("Max W.", "max@example.com");
pub(crate) const OLA: (&str, &str) = ("Ola", "ola@example.com");

pub(crate) fn stats_at(dir: &Path, window: Window) -> RepoStats {
    let repo = Repo::open(dir).unwrap();
    let opts = StatsOptions {
        now: NOW,
        ..StatsOptions::default()
    };
    repo.stats_with(window, &opts, &AtomicBool::new(false))
        .unwrap()
}

/// The reference project, on `main` (see the history in the comments):
///
/// ```text
/// c1 100d Richard chore: init      (.mailmap, a.txt, Cargo.lock, CHANGELOG.md)
/// c2  45d Max     feat: add b      <- tag v1
/// c3  10d Max W.  fix(core): bug
/// M    8d Richard Merge branch 'done'   (d1 9d Richard feat: done work)
/// c4   3d Richard docs: readme     (empties a.txt: one line removed)
/// c5   2d Richard feat!: breaking       <- main, `same`
/// old:     o1 80d Ola     wip: stuff        (off c2)
/// feature: f1 5d Max W.   feat: f1          (off c3)
///          f2 4d Richard  test: f2
/// ```
///
/// plus a stash, and a dirty tree: `a.txt` changed, `b.txt` staged, `u.txt` new.
pub(crate) fn project() -> TempDir {
    let tmp = TempDir::new("stats-project");
    let dir = tmp.path();
    init(dir, "main");
    let mailmap =
        "Max Wells <max@example.com> <max@old.example.com>\nMax Wells <max@example.com>\n";
    commit(
        dir,
        RICHARD,
        ago(100),
        "chore: init",
        &[
            (".mailmap", mailmap),
            ("a.txt", "a\n"),
            ("Cargo.lock", "l1\n"),
            ("CHANGELOG.md", "c1\n"),
        ],
    );
    let c2 = commit(
        dir,
        MAX_OLD,
        ago(45),
        "feat: add b",
        &[
            ("b.txt", "b\n"),
            ("src/app.rs", "x\n"),
            ("Cargo.lock", "l1\nl2\n"),
            ("CHANGELOG.md", "c1\nc2\n"),
        ],
    );
    git(dir, &["tag", "v1"]);
    git(dir, &["checkout", "-q", "-b", "old", &c2]);
    commit(dir, OLA, ago(80), "wip: stuff", &[("o.txt", "o\n")]);
    git(dir, &["checkout", "-q", "main"]);
    let c3 = commit(
        dir,
        MAX_NEW,
        ago(10),
        "fix(core): bug",
        &[("c.txt", "c\n"), ("src/app.rs", "x\ny\n")],
    );
    git(dir, &["checkout", "-q", "-b", "done"]);
    commit(
        dir,
        RICHARD,
        ago(9),
        "feat: done work",
        &[("d.txt", "d\n"), ("src/app.rs", "x\ny\nz\n")],
    );
    git(dir, &["checkout", "-q", "main"]);
    git_as(
        dir,
        ("Richard", "richard@example.com", ago(8)),
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Merge branch 'done'",
            "done",
        ],
    );
    git(dir, &["checkout", "-q", "-b", "feature", &c3]);
    commit(dir, MAX_NEW, ago(5), "feat: f1", &[("f.txt", "f\n")]);
    commit(dir, RICHARD, ago(4), "test: f2", &[("f.txt", "f\nf\n")]);
    git(dir, &["checkout", "-q", "main"]);
    commit(
        dir,
        RICHARD,
        ago(3),
        "docs: readme",
        &[("README.md", "r\n")],
    );
    commit(dir, RICHARD, ago(2), "feat!: breaking", &[("e.txt", "e\n")]);
    git(dir, &["branch", "same"]);
    fs::write(dir.join("a.txt"), "stashed\n").unwrap();
    git(dir, &["stash", "push", "-q"]);
    fs::write(dir.join("a.txt"), "changed\n").unwrap();
    fs::write(dir.join("b.txt"), "b staged\n").unwrap();
    git(dir, &["add", "b.txt"]);
    fs::write(dir.join("u.txt"), "u\n").unwrap();
    tmp
}

pub(crate) fn buckets(stats: &RepoStats) -> Vec<usize> {
    stats.series.iter().map(|b| b.commits).collect()
}

pub(crate) const RICHARD_WORK: (&str, &str) = ("Richard", "richard@work.example.com");

pub(crate) const RICHARD_OLD: (&str, &str) = ("richard ", "Richard@Old.example.com");
