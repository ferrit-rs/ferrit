#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test: a failed setup or a bad slice is the assertion"
)]
//! Mechanism 2 of `docs/PLAN_SELF_TESTING.md`, the correctness gate: run every
//! script under `test/scripts/` against its fixture. A failure names the
//! script and line and prints the frame it saw; every run also leaves its
//! snapshots under `target/tmp/replay/<script>/` for a human or an agent to
//! read.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use ferrit::replay::runner::{self, Options};
use ferrit::replay::script::{self, Directive};

fn scripts() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test/scripts");
    let mut found: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "script"))
        .collect();
    found.sort();
    found
}

fn stem(path: &Path) -> String {
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

/// Where a run's frames go. `CARGO_TARGET_TMPDIR` is per-workspace and cleaned
/// with `cargo clean`.
fn frames_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("replay")
        .join(name)
}

#[test]
fn every_script_passes() {
    let scripts = scripts();
    assert!(!scripts.is_empty(), "test/scripts has no scripts");

    let mut failures = String::new();
    for path in &scripts {
        let name = stem(path);
        let text = fs::read_to_string(path).unwrap();
        let script = match script::parse(&text) {
            Ok(script) => script,
            Err(error) => {
                writeln!(failures, "\n{name}: does not parse: {error}").unwrap();
                continue;
            },
        };
        // A script that never checks anything proves nothing.
        let asserts = script.steps.iter().any(|step| {
            matches!(
                step.directive,
                Directive::ExpectText(_) | Directive::ExpectNoText(_) | Directive::Git { .. }
            )
        });
        if !asserts {
            writeln!(
                failures,
                "\n{name}: asserts nothing (no expect-text, expect-no-text or git check)"
            )
            .unwrap();
            continue;
        }

        let out = frames_dir(&name);
        let _ = fs::remove_dir_all(&out);
        match runner::run(&script, &Options::default()) {
            Ok(outcome) => {
                fs::create_dir_all(&out).unwrap();
                for frame in &outcome.frames {
                    fs::write(
                        out.join(format!("{:03}-{}.txt", frame.index, frame.label)),
                        &frame.text,
                    )
                    .unwrap();
                }
            },
            Err(failure) => {
                fs::create_dir_all(&out).unwrap();
                fs::write(out.join("failure.txt"), &failure.frame).unwrap();
                writeln!(
                    failures,
                    "\n{name}: {failure}\n(the frame is in {})\n{}",
                    out.join("failure.txt").display(),
                    failure.frame
                )
                .unwrap();
            },
        }
    }
    assert!(failures.is_empty(), "{failures}");
}

#[test]
fn the_scripts_cover_each_named_flow() {
    // The flows the plans promised a script for. Deleting one fails here, not
    // silently.
    let names: Vec<String> = scripts().iter().map(|p| stem(p)).collect();
    for wanted in [
        "10-layout",
        "20-status-files",
        "30-diff",
        "35-scroll",
        "40-stage",
        "50-commit",
        "60-branches",
        "70-remote",
        "80-stash",
        "65-merge",
        "90-rewrite",
        "91-operation",
        "92-fixup",
        "95-conflict",
        "100-command-log",
        "110-keymap",
        "120-context-menu",
        "121-take-side",
        "130-theme",
    ] {
        assert!(
            names.iter().any(|n| n == wanted),
            "test/scripts/{wanted}.script is missing"
        );
    }
}

#[test]
fn every_script_name_is_numbered_and_unique() {
    let names: Vec<String> = scripts().iter().map(|p| stem(p)).collect();
    let mut seen = std::collections::BTreeSet::new();
    for name in &names {
        let number = name.split('-').next().unwrap();
        assert!(
            number.chars().all(|c| c.is_ascii_digit()) && !number.is_empty(),
            "{name}"
        );
        assert!(
            seen.insert(number.to_owned()),
            "two scripts share the number {number}"
        );
    }
}
