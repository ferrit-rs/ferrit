#![allow(
    clippy::unwrap_used,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The binary's start-up contract (`docs/PLAN_16_START_WITHOUT_REPO.md`, W3):
//! a path named with `--path` that is not a repository is a one-line error and a
//! non-zero exit, before any terminal is taken. (The no-argument start opens the
//! welcome screen, which needs a terminal: `tests/app_welcome.rs` and the replay
//! script cover it.)

use std::fs;
use std::process::Command;

#[test]
fn a_named_path_that_is_not_a_repository_is_a_one_line_error_and_exit_1() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("ferrit-cli-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_ferrit"))
        .arg("--path")
        .arg(&dir)
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(
        stderr.starts_with("ferrit: not a git repository: "),
        "{stderr}"
    );
    assert!(out.stdout.is_empty(), "nothing is drawn");
}
