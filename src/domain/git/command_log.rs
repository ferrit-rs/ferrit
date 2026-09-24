//! A process-wide record of the `git` subprocesses ferrit ran, newest last.
//!
//! Global on purpose: the diff and refresh workers each open their own `Repo`
//! on their own thread, so a log owned by one `Repo` would never see their
//! commands. Only `exec` writes it; the UI reads it through `recent`.
//! See `docs/PLAN_12_POLISH.md` P0.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

/// Entries kept; older ones are dropped.
const CAPACITY: usize = 200;

/// Subcommands that only read. `git diff` runs on every selection change, so
/// showing them by default would bury the commands the user asked for.
const READ_ONLY: [&str; 9] = [
    "diff",
    "show",
    "rev-list",
    "log",
    "cat-file",
    "ls-files",
    "rev-parse",
    "for-each-ref",
    "status",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Read,
    Write,
}

/// One finished (or failed to start) `git` subprocess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRecord {
    /// `git checkout -b wip/x`: the working directory flag is left out and
    /// credentials in URLs are redacted.
    pub argv: String,
    pub kind: CommandKind,
    /// `None` when git could not be spawned, was killed, or was cancelled.
    pub exit: Option<i32>,
    pub took: Duration,
}

impl CommandRecord {
    pub fn failed(&self) -> bool {
        self.exit != Some(0)
    }
}

fn ring() -> &'static Mutex<VecDeque<CommandRecord>> {
    static RING: OnceLock<Mutex<VecDeque<CommandRecord>>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

/// Append a record, dropping the oldest at capacity. A poisoned lock is
/// recovered: a panic in one worker must not blind the log.
pub(super) fn record(entry: CommandRecord) {
    let mut ring = ring().lock().unwrap_or_else(PoisonError::into_inner);
    if ring.len() == CAPACITY {
        ring.pop_front();
    }
    ring.push_back(entry);
}

/// The last `count` records, oldest first. Reads are left out unless
/// `include_reads`.
pub fn recent(count: usize, include_reads: bool) -> Vec<CommandRecord> {
    let mut picked: Vec<CommandRecord> = ring()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .rev()
        .filter(|entry| include_reads || entry.kind == CommandKind::Write)
        .take(count)
        .cloned()
        .collect();
    picked.reverse();
    picked
}

/// Read or write, from the subcommand and, for `stash`, its verb.
pub(super) fn classify(subcommand: &str, verb: Option<&str>) -> CommandKind {
    if READ_ONLY.contains(&subcommand) || (subcommand == "stash" && verb == Some("show")) {
        CommandKind::Read
    } else {
        CommandKind::Write
    }
}

/// `scheme://user:secret@host/path` becomes `scheme://user:***@host/path`.
/// Anything without credentials is returned unchanged.
pub(super) fn redact(arg: &str) -> String {
    let Some(scheme_end) = arg.find("://") else {
        return arg.to_owned();
    };
    let (scheme, rest) = arg.split_at(scheme_end + 3);
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let Some(at) = rest.get(..authority_end).and_then(|a| a.rfind('@')) else {
        return arg.to_owned();
    };
    let (credentials, host) = rest.split_at(at);
    match credentials.split_once(':') {
        Some((user, _secret)) => format!("{scheme}{user}:***{host}"),
        None => arg.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandKind, classify, redact};

    #[test]
    fn redact_hides_a_password_and_keeps_the_rest() {
        assert_eq!(
            redact("https://me:s3cret@example.com/o.git"),
            "https://me:***@example.com/o.git"
        );
    }

    #[test]
    fn redact_leaves_urls_without_a_password_alone() {
        assert_eq!(
            redact("https://example.com/o.git"),
            "https://example.com/o.git"
        );
        assert_eq!(
            redact("ssh://git@example.com/o.git"),
            "ssh://git@example.com/o.git"
        );
        assert_eq!(redact("checkout"), "checkout");
        assert_eq!(redact("user:pass@host"), "user:pass@host");
    }

    #[test]
    fn redact_only_looks_at_the_authority() {
        assert_eq!(
            redact("https://example.com/a:b@c"),
            "https://example.com/a:b@c"
        );
    }

    #[test]
    fn classify_separates_reads_from_writes() {
        assert_eq!(classify("diff", None), CommandKind::Read);
        assert_eq!(classify("stash", Some("show")), CommandKind::Read);
        assert_eq!(classify("stash", Some("push")), CommandKind::Write);
        assert_eq!(classify("checkout", None), CommandKind::Write);
    }
}
