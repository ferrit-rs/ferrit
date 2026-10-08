//! Helpers shared by the integration tests. Every file in `tests/` is its own
//! crate, so each one that needs these declares `mod common;` and uses what it
//! wants; the rest of the module is dead code in that crate, hence the allow.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "shared test scaffolding: each test crate uses a part of it, and a failed setup is the assertion"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use git2::{IndexAddOption, Repository, Signature};

/// A fresh directory under the system temp dir, removed when dropped. The name
/// carries `tag`, the process id, the clock and a counter, so tests running in
/// one process or in parallel never share one.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "ferrit-{tag}-{}-{nanos}-{count}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    /// A directory called `name` inside this one (it may hold spaces or
    /// quotes), removed on its own drop as well.
    pub(crate) fn child(&self, name: &str) -> Self {
        let path = self.0.join(name);
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

/// `git -C dir args...`, its trimmed stdout. Panics with git's stderr on failure.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Give the repository at `dir` an identity of its own, so a commit never
/// depends on the user's global config.
pub(crate) fn configure_identity(dir: &Path) {
    for (key, value) in [("user.name", "Test"), ("user.email", "test@example.com")] {
        git(dir, &["config", key, value]);
    }
}

/// Commit everything in the worktree, on top of `HEAD` when there is one.
pub(crate) fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let parent = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .and_then(|oid| repo.find_commit(oid).ok());
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}
