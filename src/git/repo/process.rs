//! Running a child process to completion with a timeout and a cancel flag:
//! piped output, stdin closed, its own process group, recorded in the command
//! log. Shared by git's network commands (`crate::git::repo::remotes`) and by
//! `gh` (`host`). No `git2` here, only `std::process`.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::git::REMOTE_TIMEOUT;
use crate::git::askpass;
use crate::git::error::{GitError, GitResult};
use crate::git::repo::exec;

const TERMINATE_GRACE: Duration = Duration::from_secs(2);

const POLL_INTERVAL: Duration = Duration::from_millis(40);

/// `stdout` + `stderr`, each trimmed, joined by a newline when both are
/// non-empty. Unlike `apply.rs`/`commit.rs`/`branch.rs`'s error-only
/// `stderr()`, a fetch/pull/push's *success* line is worth showing too
/// (git puts progress and the human summary on stderr, machine-parseable
/// bits, when there are any, on stdout); ferrit does not parse either,
/// just shows them.
pub(crate) fn combined_output(out: &Output) -> String {
    let out_text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let err_text = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    match (out_text.is_empty(), err_text.is_empty()) {
        (true, _) => err_text,
        (false, true) => out_text,
        (false, false) => format!("{out_text}\n{err_text}"),
    }
}

/// `git -C <workdir> <...args>`, run to completion. `Ok`/`Err` both carry
/// `combined_output`; the caller's `err` only decides which `GitError`
/// variant wraps a non-zero exit.
pub(crate) fn run_git(
    workdir: &Path,
    args: &[String],
    cancel: Option<&AtomicBool>,
    err: impl Fn(String) -> GitError,
) -> GitResult<String> {
    let out = run_command(workdir, args, cancel, &err)?;
    let combined = combined_output(&out);
    if out.status.success() {
        Ok(combined)
    } else {
        Err(err(combined))
    }
}

pub(crate) fn run_command(
    workdir: &Path,
    args: &[String],
    cancel: Option<&AtomicBool>,
    err: &impl Fn(String) -> GitError,
) -> GitResult<Output> {
    let mut command = exec::git(workdir);
    command.args(args);
    askpass::configure(&mut command);
    run_child(command, "git", REMOTE_TIMEOUT, cancel, err)
}

/// Run an already-built `command` to completion: piped output, stdin closed,
/// its own process group, recorded in the command log, stopped when `cancel`
/// is set or `timeout` passes. `name` is how the program is called in the
/// messages (`git`, `gh`). Shared by git's network commands and `gh`
/// (`git::host`).
pub(crate) fn run_child(
    mut command: Command,
    name: &str,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
    err: &impl Fn(String) -> GitError,
) -> GitResult<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let tracked = exec::track(&command);
    let mut child = command
        .spawn()
        .map_err(|e| err(format!("cannot run {name}: {e}")))?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or_else(|| {
        stop_process_group(&mut child, pid);
        err(format!("cannot capture {name} stdout"))
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        stop_process_group(&mut child, pid);
        err(format!("cannot capture {name} stderr"))
    })?;
    let stdout_reader = thread::spawn(move || read_all(stdout));
    let stderr_reader = thread::spawn(move || read_all(stderr));
    let deadline = Instant::now() + timeout;

    let status = loop {
        if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            stop_process_group(&mut child, pid);
            let output = collect_output(stdout_reader, stderr_reader, name);
            return Err(err(with_diagnostics("cancelled during shutdown", &output)));
        }
        if Instant::now() >= deadline {
            stop_process_group(&mut child, pid);
            let output = collect_output(stdout_reader, stderr_reader, name);
            let reason = format!("timed out after {} seconds", timeout.as_secs());
            return Err(err(with_diagnostics(&reason, &output)));
        }
        match child.try_wait() {
            Err(e) => {
                stop_process_group(&mut child, pid);
                return Err(err(format!("cannot wait for {name}: {e}")));
            },
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(POLL_INTERVAL),
        }
    };
    tracked.finish(status.code());
    let stdout = stdout_reader
        .join()
        .map_err(|_| err(format!("{name} stdout reader panicked")))?
        .map_err(|e| err(format!("cannot read {name} stdout: {e}")))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| err(format!("{name} stderr reader panicked")))?
        .map_err(|e| err(format!("cannot read {name} stderr: {e}")))?;
    let out = Output {
        status,
        stdout,
        stderr,
    };
    Ok(out)
}

fn read_all(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader.read_to_end(&mut output)?;
    Ok(output)
}

fn collect_output(
    stdout_reader: JoinHandle<io::Result<Vec<u8>>>,
    stderr_reader: JoinHandle<io::Result<Vec<u8>>>,
    name: &str,
) -> String {
    let stdout = read_output(stdout_reader, "stdout", name);
    let stderr = read_output(stderr_reader, "stderr", name);
    [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_output(reader: JoinHandle<io::Result<Vec<u8>>>, stream: &str, name: &str) -> String {
    match reader.join() {
        Ok(Ok(bytes)) => String::from_utf8_lossy(&bytes).trim().to_owned(),
        Ok(Err(error)) => format!("cannot read {name} {stream}: {error}"),
        Err(_) => format!("{name} {stream} reader panicked"),
    }
}

fn with_diagnostics(reason: &str, output: &str) -> String {
    if output.is_empty() {
        reason.to_owned()
    } else {
        format!("{reason}\n{output}")
    }
}

fn stop_process_group(child: &mut Child, pid: u32) {
    signal_process_group(pid, false);
    let deadline = Instant::now() + TERMINATE_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
            _ => break,
        }
    }
    signal_process_group(pid, true);
    #[cfg(not(unix))]
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn signal_process_group(pid: u32, force: bool) {
    let signal = if force { "-KILL" } else { "-TERM" };
    // Use the system utility: this crate forbids unsafe code. The negative
    // pid targets the process group created for the Git command.
    let _ = Command::new("/bin/kill")
        .arg(signal)
        .arg(format!("-{pid}"))
        .status();
}

#[cfg(not(unix))]
fn signal_process_group(_pid: u32, _force: bool) {}
