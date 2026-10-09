//! The one place a `git` subprocess is built, run and recorded, so the
//! command log (`command_log`) sees every command. See
//! `docs/PLAN_12_POLISH.md` P0.
//!
//! Sites differ in how they drive the child (`output` to completion, piped
//! stdin, cancellable polling), so recording is separate from running:
//! `output` runs and records in one call, `track` records a command the
//! caller manages itself.

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Instant;

use crate::git::command_log::{self, CommandRecord, classify, redact};

/// `git -C <workdir>`, ready for the caller to add arguments. The only
/// constructor of a git `Command` in this crate (`tests/git_exec.rs` scans
/// the sources to keep it that way).
pub(crate) fn git(workdir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(workdir);
    cmd
}

/// A command for an external program other than git (`gh`), ready for the
/// caller to add arguments. Built here, beside `git()`, so the command log sees
/// it too: `track` records it under the program's own name.
pub(crate) fn program(program: &OsStr) -> Command {
    Command::new(program)
}

/// Run `cmd` to completion, capturing its output, and record it.
pub(crate) fn output(cmd: &mut Command) -> io::Result<Output> {
    let tracked = track(cmd);
    let out = cmd.output();
    match out.as_ref() {
        Ok(o) => tracked.finish_with_stdout(o.status.code(), &o.stdout),
        Err(_) => tracked.finish(None),
    }
    out
}

/// Start timing `cmd`. The record is written when the returned value is
/// dropped, with the exit code given to `finish` or `None` if it never was
/// (spawn failure, early return, cancellation), so a command is recorded
/// exactly once on every path.
pub(crate) fn track(cmd: &Command) -> Tracked {
    // A program other than git is recorded as it is: no `-C` pair to drop, no
    // config secrets to mask.
    let name = Path::new(cmd.get_program())
        .file_name()
        .map_or_else(|| "git".to_owned(), |n| n.to_string_lossy().into_owned());
    if name != "git" {
        let rest: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let kind = command_log::classify_program(&rest);
        let argv = std::iter::once(name)
            .chain(rest.iter().map(|a| redact(a)))
            .collect::<Vec<_>>()
            .join(" ");
        return Tracked {
            argv,
            kind,
            started: Instant::now(),
            exit: None,
            output: Vec::new(),
        };
    }
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
        .chain(
            command_log::mask_config_secrets(rest)
                .iter()
                .map(|a| redact(a)),
        )
        .collect::<Vec<_>>()
        .join(" ");
    Tracked {
        argv,
        kind,
        started: Instant::now(),
        exit: None,
        output: Vec::new(),
    }
}

/// A command being timed; see `track`.
pub(crate) struct Tracked {
    argv: String,
    kind: command_log::CommandKind,
    started: Instant,
    exit: Option<i32>,
    output: Vec<String>,
}

impl Tracked {
    /// Set the exit code and record now.
    pub(crate) fn finish(mut self, exit: Option<i32>) {
        self.exit = exit;
    }

    /// `finish`, and keep the lines git wrote on stdout (up to
    /// `MAX_OUTPUT_LINES`) when this is a write: the answer the command log
    /// shows under the command.
    pub(crate) fn finish_with_stdout(mut self, exit: Option<i32>, stdout: &[u8]) {
        self.exit = exit;
        if self.kind == command_log::CommandKind::Write {
            self.output = String::from_utf8_lossy(stdout)
                .lines()
                .map(str::trim_end)
                .filter(|line| !line.is_empty())
                .take(command_log::MAX_OUTPUT_LINES)
                .map(str::to_owned)
                .collect();
        }
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        command_log::record(CommandRecord {
            argv: std::mem::take(&mut self.argv),
            kind: self.kind,
            exit: self.exit,
            took: self.started.elapsed(),
            output: std::mem::take(&mut self.output),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::output;
    use crate::git::command_log::recent;

    #[test]
    fn a_write_keeps_its_non_empty_stdout_lines_up_to_the_cap() {
        let mut cmd = super::git(std::path::Path::new("."));
        // `var -l` is a write to the log and prints one line per config entry.
        cmd.args(["-c", "zz.marker=1", "var", "-l"]);
        assert!(output(&mut cmd).is_ok());
        let records: Vec<_> = recent(usize::MAX, true)
            .into_iter()
            .filter(|entry| entry.argv.contains("zz.marker=1"))
            .collect();
        assert_eq!(records.len(), 1, "{records:?}");
        let lines: Vec<_> = records.iter().flat_map(|r| r.output.clone()).collect();
        assert!(lines.len() > 1, "more than the first line");
        assert!(lines.len() <= crate::git::command_log::MAX_OUTPUT_LINES);
        assert!(lines.iter().all(|line| !line.is_empty()));
    }

    #[test]
    fn another_program_is_recorded_under_its_own_name() {
        let mut cmd = super::program(std::ffi::OsStr::new("true"));
        cmd.args(["auth", "status", "zz-program-marker"]);
        assert!(output(&mut cmd).is_ok());
        let records: Vec<_> = recent(usize::MAX, true)
            .into_iter()
            .filter(|entry| entry.argv.contains("zz-program-marker"))
            .collect();
        assert_eq!(records.len(), 1, "{records:?}");
        let argv = records.first().map(|r| r.argv.as_str());
        assert_eq!(argv, Some("true auth status zz-program-marker"));
        assert_eq!(records.first().and_then(|r| r.exit), Some(0));
    }

    #[test]
    fn a_command_that_cannot_be_spawned_is_recorded_with_no_exit_code() {
        let mut cmd = Command::new("ferrit-no-such-binary");
        cmd.arg("zz-unspawnable-marker");
        assert!(output(&mut cmd).is_err(), "the spawn failed");

        let entries: Vec<_> = recent(usize::MAX, true)
            .into_iter()
            .filter(|entry| entry.argv.contains("zz-unspawnable-marker"))
            .collect();
        assert!(
            matches!(entries.as_slice(), [entry] if entry.exit.is_none() && entry.failed()),
            "one record, no exit code, counted as a failure: {entries:?}"
        );
    }
}
