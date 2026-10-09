#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The dependency rule, checked on the sources. Inside `git`, only `git/repo`
//! (the adapter) names `git2`, and nothing but `git/image` (which draws the
//! preview) and `git/keys` (the glue with `App`, keys in and errors out)
//! knows the terminal or the app. `app` reaches the adapter only where it builds
//! the app (`App::open`) and starts a repository (`git init`), and the adapter
//! does not reach into `app`. See `docs/architecture.md` and
//! `docs/PLAN_21_GIT_PORT.md`.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    out
}

/// The files under `src/<dir>` with a line of code (not a comment) that
/// contains `needle`.
fn files_with_code(dir: &str, needle: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    rust_files(&root.join(dir))
        .into_iter()
        .filter(|path| {
            fs::read_to_string(path)
                .unwrap()
                .lines()
                .any(|line| !line.trim_start().starts_with("//") && line.contains(needle))
        })
        .map(|path| path.strip_prefix(&root).unwrap().display().to_string())
        .collect()
}

/// Like `files_with_code`, but leaving out the files under `src/<skip>`.
fn files_with_code_except(dir: &str, skip: &[&str], needle: &str) -> Vec<String> {
    files_with_code(dir, needle)
        .into_iter()
        .filter(|path| !skip.iter().any(|skip| path.starts_with(skip)))
        .collect()
}

#[test]
fn only_the_adapter_names_git2() {
    let offenders = files_with_code_except("git", &["git/repo"], "git2");
    assert!(
        offenders.is_empty(),
        "git2 used outside git/repo: {offenders:?}"
    );
}

#[test]
fn the_git_code_knows_nothing_about_the_terminal() {
    let offenders = files_with_code_except("git", &["git/image", "git/keys"], "ratatui");
    assert!(offenders.is_empty(), "ratatui used in {offenders:?}");
    let offenders = files_with_code_except("git", &["git/image", "git/keys"], "crossterm");
    assert!(offenders.is_empty(), "crossterm used in {offenders:?}");
}

#[test]
fn only_the_composition_root_and_git_init_name_the_adapter() {
    let mut offenders: Vec<String> = files_with_code("", "git::repo")
        .into_iter()
        .filter(|path| !path.starts_with("git/repo") && path != "git/mod.rs")
        .collect();
    offenders.sort();
    assert_eq!(
        offenders,
        ["app/mod.rs", "git/keys/welcome.rs"],
        "everything else goes through the GitPort traits"
    );
}

#[test]
fn the_git_domain_does_not_reach_into_the_app_except_through_its_keys() {
    let offenders = files_with_code_except("git", &["git/keys"], "crate::app");
    assert!(offenders.is_empty(), "git uses app in {offenders:?}");
}
