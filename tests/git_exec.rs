#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    clippy::iter_on_single_items,
    clippy::format_collect,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion, helper ergonomics beat lint-cleanliness here"
)]
//! The command log (`docs/PLAN_12_POLISH.md` P0): every `git` subprocess is
//! recorded through `exec`, reads are hidden by default, credentials are
//! redacted, and the ring keeps the last 200.
//!
//! The log is process-wide and these tests run in parallel, so each one looks
//! for entries carrying a name only it uses, never for "the last entry".

mod common;

use common::{TempDir, commit_all, configure_identity, git};
use std::fs;
use std::path::{Path, PathBuf};

use ferrit::git::command_log::{CommandKind, CommandRecord, recent};
use ferrit::git::diff::{DiffOpts, DiffSide};
use ferrit::git::repo::Repo;
use git2::Repository;

fn fixture(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    dir
}

/// Entries whose command line contains `needle`, oldest first.
fn logged(needle: &str) -> Vec<CommandRecord> {
    recent(usize::MAX, true)
        .into_iter()
        .filter(|entry| entry.argv.contains(needle))
        .collect()
}

#[test]
fn a_write_is_recorded_without_the_workdir_flag() {
    let dir = fixture("exec-write");
    let repo = Repo::open(dir.path()).unwrap();
    repo.create_branch("exec-write-branch").unwrap();

    let entries = logged("exec-write-branch");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].argv, "git checkout -b exec-write-branch");
    assert_eq!(entries[0].kind, CommandKind::Write);
    assert_eq!(entries[0].exit, Some(0));
    assert!(!entries[0].failed());
}

#[test]
fn a_failing_command_is_recorded_with_its_exit_code() {
    let dir = fixture("exec-fail");
    let repo = Repo::open(dir.path()).unwrap();
    assert!(repo.checkout("exec-fail-no-such-branch").is_err());

    let entries = logged("exec-fail-no-such-branch");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].failed());
    assert_ne!(entries[0].exit, Some(0));
    assert!(entries[0].exit.is_some(), "it ran, it just failed");
}

#[test]
fn reads_are_hidden_unless_asked_for() {
    let dir = fixture("exec-read");
    fs::write(dir.path().join("exec-read-file.txt"), "x\n").unwrap();
    let repo = Repo::open(dir.path()).unwrap();
    repo.file_diff(
        Path::new("exec-read-file.txt"),
        DiffSide::Worktree,
        DiffOpts::default(),
    )
    .unwrap();

    let entries = logged("exec-read-file.txt");
    assert!(!entries.is_empty(), "the diff ran through exec");
    assert!(entries.iter().all(|e| e.kind == CommandKind::Read));
    let shown = recent(usize::MAX, false);
    assert!(
        shown.iter().all(|e| !e.argv.contains("exec-read-file.txt")),
        "reads stay out of the default view"
    );
}

#[test]
fn credentials_in_a_url_never_reach_the_log() {
    let dir = fixture("exec-redact");
    let repo = Repo::open(dir.path()).unwrap();
    let _ = repo.checkout("https://me:hunter2@exec-redact.example/o.git");

    let entries = logged("exec-redact.example");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].argv.contains("me:***@"), "{}", entries[0].argv);
    assert!(!entries[0].argv.contains("hunter2"), "{}", entries[0].argv);
}

#[test]
fn a_stdin_fed_command_is_recorded_too() {
    // `commit` pipes the message through stdin and manages the child itself.
    let dir = fixture("exec-stdin");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    let repo = Repo::open(dir.path()).unwrap();
    repo.commit(
        &ferrit::git::commit::CommitKind::Normal,
        "exec-stdin message",
        ferrit::git::commit::CommitOpts::default(),
    )
    .unwrap();

    let commits: Vec<_> = recent(usize::MAX, true)
        .into_iter()
        .filter(|e| e.argv.starts_with("git commit"))
        .collect();
    assert!(!commits.is_empty(), "the commit subprocess was recorded");
    assert!(commits.iter().all(|e| e.kind == CommandKind::Write));
    assert!(commits.iter().any(|e| e.exit == Some(0)));
}

#[test]
fn a_network_command_is_recorded_too() {
    // `fetch` is spawned with process-group handling and a polling loop.
    let dir = fixture("exec-fetch");
    let repo = Repo::open(dir.path()).unwrap();
    let _ = repo.fetch(Some("exec-fetch-no-such-remote"));

    let entries = logged("exec-fetch-no-such-remote");
    assert!(!entries.is_empty(), "the fetch subprocess was recorded");
    assert!(entries[0].argv.starts_with("git fetch"), "{entries:?}");
}

#[test]
fn the_log_keeps_only_the_newest_two_hundred() {
    let dir = fixture("exec-ring");
    let repo = Repo::open(dir.path()).unwrap();
    for i in 0..250 {
        let _ = repo.checkout(&format!("exec-ring-{i:03}"));
    }

    let all = recent(usize::MAX, true);
    assert!(all.len() <= 200, "{} entries", all.len());
    assert!(logged("exec-ring-000").is_empty(), "the oldest was dropped");
    assert_eq!(logged("exec-ring-249").len(), 1, "the newest is kept");
}

#[test]
fn recent_returns_the_newest_entries_oldest_first() {
    let dir = fixture("exec-order");
    let repo = Repo::open(dir.path()).unwrap();
    let _ = repo.checkout("exec-order-a");
    let _ = repo.checkout("exec-order-b");

    let ours: Vec<String> = recent(usize::MAX, true)
        .into_iter()
        .filter(|e| e.argv.contains("exec-order-"))
        .map(|e| e.argv)
        .collect();
    assert_eq!(
        ours,
        vec!["git checkout exec-order-a", "git checkout exec-order-b"]
    );
    let two = recent(2, true);
    assert!(two.len() <= 2);
}

/// Nothing may build a `git` command or drive a child without `exec`, or the
/// log would silently miss it.
#[test]
fn a_command_that_never_completed_reads_as_an_error_line() {
    use ferrit::theme::palette::Palette;
    let record = CommandRecord {
        argv: "git zz-never-completed".to_owned(),
        kind: CommandKind::Write,
        exit: None,
        took: std::time::Duration::ZERO,
        output: Vec::new(),
    };
    let line = ferrit::interface::panes::row_lines::command_line(&Palette::DARK, &record);
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.ends_with("(not completed)"), "{text}");
    assert!(
        line.spans
            .iter()
            .skip(1)
            .all(|s| s.style.fg == Some(Palette::DARK.del)),
        "the command and its note are in the error colour: {line:?}"
    );
}

/// The only program the file starts is `delta`.
fn only_spawns_delta(text: &str) -> bool {
    text.matches("Command::new(").count() == text.matches("Command::new(\"delta\")").count()
}

#[test]
fn every_git_subprocess_goes_through_exec() {
    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );

    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        // The replay harness builds and inspects fixture repositories; it does
        // not operate one for the user, so its `git` calls are not the
        // command log's business (`src/replay/mod.rs`). Nothing else is exempt.
        if file.starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/replay")) {
            continue;
        }
        let is_exec = file.ends_with("git/exec.rs");
        if !is_exec {
            assert!(
                !text.contains("Command::new(\"git\")"),
                "{} builds a git command outside exec::git",
                file.display()
            );
        }
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        if file.starts_with(manifest.join("src/git"))
            && !is_exec
            && !file.ends_with("command_log.rs")
            && (text.contains(".output()") || text.contains(".spawn()"))
            // `delta` renders a diff for the screen; it is not a git command, so
            // it has no business in the command log.
            && !only_spawns_delta(&text)
        {
            assert!(
                text.contains("exec::"),
                "{} runs a child without exec::output or exec::track",
                file.display()
            );
        }
    }
}
