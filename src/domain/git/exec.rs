//! The one place a `git` subprocess is built, run and recorded, so the
//! command log (`command_log`) sees every command. See
//! `docs/PLAN_12_POLISH.md` P0.
//!
//! Sites differ in how they drive the child (`output` to completion, piped
//! stdin, cancellable polling), so recording is separate from running:
//! `output` runs and records in one call, `track` records a command the
//! caller manages itself.

use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Instant;

use crate::domain::git::command_log::{self, CommandRecord, classify, redact};

/// `git -C <workdir>`, ready for the caller to add arguments. The only
/// constructor of a git `Command` in this crate (`tests/git_exec.rs` scans
/// the sources to keep it that way).
pub(super) fn git(workdir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(workdir);
    cmd
}

/// Run `cmd` to completion, capturing its output, and record it.
pub(super) fn output(cmd: &mut Command) -> io::Result<Output> {
    let tracked = track(cmd);
    let out = cmd.output();
    tracked.finish(out.as_ref().ok().and_then(|o| o.status.code()));
    out
}

/// Start timing `cmd`. The record is written when the returned value is
/// dropped, with the exit code given to `finish` or `None` if it never was
/// (spawn failure, early return, cancellation), so a command is recorded
/// exactly once on every path.
pub(super) fn track(cmd: &Command) -> Tracked {
    let mut args = cmd.get_args().map(|a| a.to_string_lossy().into_owned());
    // Drop the `-C <workdir>` pair `git()` adds: noise in a log line.
    let first = args.next();
    let rest: Vec<String> = if first.as_deref() == Some("-C") {
        args.skip(1).collect()
    } else {
        first.into_iter().chain(args).collect()
    };
    let kind = classify(
        rest.first().map_or("", String::as_str),
        rest.get(1).map(String::as_str),
    );
    let argv = std::iter::once("git".to_owned())
        .chain(rest.iter().map(|a| redact(a)))
        .collect::<Vec<_>>()
        .join(" ");
    Tracked {
        argv,
        kind,
        started: Instant::now(),
        exit: None,
    }
}

/// A command being timed; see `track`.
pub(super) struct Tracked {
    argv: String,
    kind: command_log::CommandKind,
    started: Instant,
    exit: Option<i32>,
}

impl Tracked {
    /// Set the exit code and record now.
    pub(super) fn finish(mut self, exit: Option<i32>) {
        self.exit = exit;
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        command_log::record(CommandRecord {
            argv: std::mem::take(&mut self.argv),
            kind: self.kind,
            exit: self.exit,
            took: self.started.elapsed(),
        });
    }
}
