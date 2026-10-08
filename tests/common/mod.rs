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
use std::sync::atomic::{AtomicU64, Ordering};

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
