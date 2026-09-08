//! `App`-level wiring for the right-pane diff: selecting a file builds a real
//! `DiffView`, a background `refresh()` of the same selection rebuilds the text
//! but keeps the scroll, and moving to another file resets the scroll to the
//! top. See `docs/PLAN_3_DIFF_VIEW.md` milestone D3.

use std::fs;
use std::path::{Path, PathBuf};

use ferrit::app::{App, DiffView, Pane};
use git2::{IndexAddOption, Repository, Signature};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let parent = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .and_then(|oid| repo.find_commit(oid).ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}

fn diff_text(app: &App) -> String {
    match app.diff_view() {
        DiffView::Files(d) | DiffView::Commit(_, d) => d.text.clone(),
        other => panic!("expected a real diff, got {other:?}"),
    }
}

/// Index of the Files row whose path ends with `name`.
fn files_row(app: &App, name: &str) -> usize {
    (0..app.row_count(Pane::Files))
        .find(|&i| app.file_display(i).ends_with(name))
        .unwrap_or_else(|| panic!("no Files row for {name}"))
}

#[test]
fn refresh_keeps_the_scroll_for_an_unchanged_selection() {
    let dir = TempDir::new("app-diff-scroll");
    let repo = Repository::init(dir.path()).unwrap();
    let long: String = (0..60).map(|n| format!("line {n}\n")).collect();
    fs::write(dir.path().join("big.txt"), &long).unwrap();
    fs::write(dir.path().join("other.txt"), "x\n").unwrap();
    commit_all(&repo, "init");

    // A change near the bottom so there is something to scroll to.
    let edited = long.replace("line 55\n", "line 55 CHANGED\n");
    fs::write(dir.path().join("big.txt"), &edited).unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "big.txt"));
    let before = diff_text(&app);
    assert!(before.contains("+line 55 CHANGED"));

    app.set_right_scroll(12);
    assert_eq!(app.right_scroll(), 12);

    // A second edit lands from "another shell"; the event loop calls refresh().
    let edited2 = edited.replace("line 10\n", "line 10 ALSO\n");
    fs::write(dir.path().join("big.txt"), &edited2).unwrap();
    app.refresh();

    assert!(diff_text(&app).contains("+line 10 ALSO"), "diff text rebuilt");
    assert_eq!(app.right_scroll(), 12, "scroll survives a refresh");
}

#[test]
fn moving_to_another_file_resets_the_scroll() {
    let dir = TempDir::new("app-diff-reset");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("a.txt"), "aaa\n").unwrap();
    fs::write(dir.path().join("b.txt"), "bbb\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "aaa changed\n").unwrap();
    fs::write(dir.path().join("b.txt"), "bbb changed\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "a.txt"));
    app.set_right_scroll(3);

    app.select(Pane::Files, files_row(&app, "b.txt"));
    assert_eq!(app.right_scroll(), 0, "new selection starts at the top");
    assert!(diff_text(&app).contains("+bbb changed"));
}
