#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! Workspace dependency rules, checked on the sources. The adapter owns Git
//! processes and `git2`; the app owns behavior; TUI primitives stay reusable.
//! See `docs/architecture.md` and `docs/PLAN_21_GIT_PORT.md`.

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
fn package_src(package: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../")
        .join(package)
        .join("src")
}

fn files_with_code(package: &str, dir: &str, needle: &str) -> Vec<String> {
    let root = package_src(package);
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
fn files_with_code_except(package: &str, dir: &str, skip: &[&str], needle: &str) -> Vec<String> {
    files_with_code(package, dir, needle)
        .into_iter()
        .filter(|path| !skip.iter().any(|skip| path.starts_with(skip)))
        .collect()
}

#[test]
fn only_the_adapter_names_git2() {
    let offenders = files_with_code_except("ferrit-git", "", &["repo"], "git2");
    assert!(
        offenders.is_empty(),
        "git2 used outside git/repo: {offenders:?}"
    );
}

#[test]
fn the_git_code_knows_nothing_about_the_terminal() {
    let offenders = files_with_code("ferrit-git", "", "ratatui");
    assert!(offenders.is_empty(), "ratatui used in {offenders:?}");
    let offenders = files_with_code("ferrit-git", "", "crossterm");
    assert!(offenders.is_empty(), "crossterm used in {offenders:?}");
}

#[test]
fn only_the_composition_root_and_git_init_name_the_adapter() {
    let mut offenders: Vec<String> = files_with_code("ferrit-app", "", "ferrit_git::repo")
        .into_iter()
        .collect();
    offenders.sort();
    assert_eq!(
        offenders,
        ["ui/mod.rs"],
        "everything else goes through the GitPort traits"
    );
}

#[test]
fn the_git_domain_does_not_know_the_app() {
    let offenders = files_with_code("ferrit-git", "", "ferrit_app");
    assert!(offenders.is_empty(), "git uses app in {offenders:?}");
}

/// A domain is a flat list of files named after what they do, with a folder only
/// for a feature that has several files: nothing deeper than two folders under
/// `src/<domain>/`, except the widgets of `ferrit-tui/widgets`.
#[test]
fn the_tree_is_at_most_two_folders_deep_under_a_domain() {
    let roots = [
        package_src("ferrit-app"),
        package_src("ferrit-git"),
        package_src("ferrit-tui"),
    ];
    let too_deep: Vec<String> = roots
        .into_iter()
        .flat_map(|root| {
            let package = root
                .parent()
                .and_then(Path::file_name)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            rust_files(&root)
                .into_iter()
                .map(move |path| (package.clone(), root.clone(), path))
        })
        .map(|(package, root, path)| (package, path.strip_prefix(root).unwrap().to_path_buf()))
        .filter(|(_, path)| path.components().count() > 4)
        .filter(|(package, path)| !(package.ends_with("ferrit-tui") && path.starts_with("widgets")))
        .map(|(package, path)| format!("{package}/{}", path.display()))
        .collect();
    assert!(too_deep.is_empty(), "too deep: {too_deep:?}");
}

/// The behaviour of `App` is written in `ui/` and nowhere else: a domain
/// defines its own types and rules and never an `impl App`.
#[test]
fn impl_app_is_only_written_in_app() {
    let root = package_src("ferrit-app");
    let offenders: Vec<String> = rust_files(&root)
        .into_iter()
        .filter(|path| !path.starts_with(root.join("ui")))
        .filter(|path| {
            fs::read_to_string(path)
                .unwrap()
                .lines()
                .any(|line| line.starts_with("impl App"))
        })
        .map(|path| path.strip_prefix(&root).unwrap().display().to_string())
        .collect();
    assert!(offenders.is_empty(), "impl App outside ui/: {offenders:?}");
}

/// A component decides and returns events; it never writes behaviour on `App`.
/// What changes the state is `ui/reducer.rs`, and the few flows that cross the
/// popups, the workers and the repository are the files of `ui/` itself.
#[test]
fn no_component_writes_an_impl_app() {
    let root = package_src("ferrit-app").join("ui");
    let offenders: Vec<String> = rust_files(&root.join("components"))
        .into_iter()
        .chain(rust_files(&package_src("ferrit-tui").join("widgets")))
        .filter(|path| {
            fs::read_to_string(path)
                .unwrap()
                .lines()
                .any(|line| line.starts_with("impl App"))
        })
        .map(|path| path.strip_prefix(&root).unwrap().display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "impl App in components: {offenders:?}"
    );
}

/// Drawing reads a `Scene` (references to the state, `ui/scene.rs`), never
/// `App`: only the entry points of `ui/draw.rs` name it.
#[test]
fn only_the_draw_entry_points_name_app_among_the_drawing_code() {
    let root = package_src("ferrit-app").join("ui");
    let offenders: Vec<String> = rust_files(&root.join("components"))
        .into_iter()
        .chain(rust_files(&package_src("ferrit-tui").join("widgets")))
        .chain(rust_files(&root.join("row_lines")))
        .filter(|path| {
            fs::read_to_string(path).unwrap().lines().any(|line| {
                let line = line.trim_start();
                !line.starts_with("//")
                    && (line.contains(": &App")
                        || line.contains("<App>")
                        || line.contains("tui::App"))
            })
        })
        .map(|path| path.strip_prefix(&root).unwrap().display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "App named in drawing code: {offenders:?}"
    );
}

/// Starting a process (`git`, `gh`) is the adapter's job: the domain files of
/// `git/` decide and describe, they do not run anything.
#[test]
fn only_the_adapter_starts_git_and_gh() {
    let offenders =
        files_with_code_except("ferrit-git", "", &["repo", "repo/diff.rs"], "Command::new");
    assert!(offenders.is_empty(), "a process started in {offenders:?}");
}
