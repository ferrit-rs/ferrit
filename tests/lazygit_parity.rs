#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! What the audit of `test/flows/feature-workflow.flow` found ferrit lacking next to
//! lazygit. Each test is one row's definition of done, on a real repository, read
//! from the rendered frame.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::{App, Pane, screens};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "ferrit-parity-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let repo = Self { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Test"),
            ("user.email", "test@example.com"),
            ("commit.gpgsign", "false"),
            ("core.editor", "true"),
        ] {
            repo.git(&["config", key, value]);
        }
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.dir.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }

    fn commit(&self, path: &str, text: &str, message: &str) {
        self.write(path, text);
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn app(&self) -> App {
        App::open(&self.dir).unwrap()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn frame(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal.draw(|f| screens::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

fn key(app: &mut App, c: char) {
    app.feed_key(KeyEvent::from(KeyCode::Char(c)));
}

/// Done when: with nothing to commit and the Files pane focused, the right pane is
/// titled "Diff" and says "No changed files", as lazygit's does (step 6, 11).
#[test]
fn an_empty_working_tree_gives_a_diff_pane_that_says_so() {
    let repo = Repo::new("empty-diff");
    repo.commit("a.txt", "one\n", "init");
    let mut app = repo.app();
    key(&mut app, '2');
    let out = frame(&mut app);
    assert!(out.contains("Diff"), "{out}");
    assert!(out.contains("No changed files"), "{out}");
    assert!(!out.contains("Unstaged changes"), "{out}");

    // With a change, the two-sided view is back.
    repo.write("a.txt", "one\ntwo\n");
    app.refresh();
    let out = frame(&mut app);
    assert!(out.contains("Unstaged Changes"), "{out}");
    assert!(!out.contains("No changed files"), "{out}");
    let _ = (Pane::Files, Path::new(""));
}

/// The foreground colour of the first cell of `text` where it appears on the frame.
fn colour_of(app: &mut App, text: &str) -> Option<ratatui::style::Color> {
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal.draw(|f| screens::draw(f, app)).unwrap();
    let cells = &terminal.backend().buffer().content;
    let wanted: Vec<String> = text.chars().map(String::from).collect();
    (0..cells.len().saturating_sub(wanted.len())).find_map(|start| {
        let matches = wanted
            .iter()
            .enumerate()
            .all(|(offset, ch)| cells[start + offset].symbol() == ch);
        matches.then(|| cells[start].fg)
    })
}

/// Done when: an untracked file is `??` in red, an unstaged change is red and a staged
/// one green, so staging a file changes its colour (steps 2, 3). The first row is the
/// selected one and takes the selection colours, so the rows compared are below it.
#[test]
fn file_markers_are_red_when_unstaged_and_green_when_staged() {
    let repo = Repo::new("markers");
    repo.commit("m.txt", "one\n", "init");
    repo.write("m.txt", "one\ntwo\n");
    repo.write("a_first.txt", "the selected row\n");
    repo.write("u.txt", "fresh\n");
    let mut app = repo.app();
    key(&mut app, '2');
    let out = frame(&mut app);
    assert!(out.contains("?? u.txt"), "untracked shows as ??\n{out}");

    let untracked = colour_of(&mut app, "?? u.txt").unwrap();
    let unstaged = colour_of(&mut app, "M m.txt").unwrap();
    assert_eq!(unstaged, untracked, "an unstaged M is red like ??");

    repo.git(&["add", "m.txt"]);
    app.refresh();
    let staged = colour_of(&mut app, "M  m.txt").unwrap();
    assert_ne!(staged, untracked, "a staged M is not red");
}

/// Done when: `Space` on a directory row stages every file under it, and the same key
/// unstages them again, as lazygit does (the stage-directory flow).
#[test]
fn space_on_a_directory_stages_and_unstages_everything_under_it() {
    let repo = Repo::new("stage-dir");
    repo.commit("README.md", "top\n", "init");
    repo.write("docs/a.md", "a\n");
    repo.write("docs/deep/b.md", "b\n");
    repo.write("outside.txt", "not under docs\n");
    let mut app = repo.app();
    key(&mut app, '2');
    let docs_row = (0..app.row_count(Pane::Files))
        .find(|&i| app.file_lines()[i].to_string().contains("docs"))
        .expect("a docs row");
    app.select(Pane::Files, docs_row);

    key(&mut app, ' ');
    app.refresh();
    let status = repo.git(&["status", "--porcelain"]);
    assert!(status.contains("A  docs/a.md"), "{status}");
    assert!(status.contains("A  docs/deep/b.md"), "{status}");
    assert!(
        status.contains("?? outside.txt"),
        "a file outside stays: {status}"
    );

    app.select(Pane::Files, docs_row);
    key(&mut app, ' ');
    app.refresh();
    let status = repo.git(&["status", "--porcelain"]);
    assert!(status.contains("?? docs/"), "unstaged again: {status}");
    assert!(!status.contains("A  docs"), "{status}");
}

/// Done when: after a commit made from ferrit, the command log's record of
/// `git commit -F -` carries git's own answer, `[main abc1234] summary`, and the Infos
/// box draws it under the command (step 6, 11). The log is process wide, so the record
/// is looked up by its unique message rather than read off a frame other tests write to.
#[test]
fn a_commit_shows_git_s_own_answer_under_the_command() {
    use ferrit::app::theme;
    use ferrit::components::ui::palette::Palette;
    use ferrit::domain::git::command_log;

    let repo = Repo::new("commit-output");
    repo.commit("a.txt", "one\n", "init");
    repo.write("a.txt", "one\ntwo\n");
    repo.git(&["add", "a.txt"]);
    let mut app = repo.app();
    key(&mut app, 'c');
    for c in "zz answer marker".chars() {
        key(&mut app, c);
    }
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    let record = command_log::recent(200, false)
        .into_iter()
        .rev()
        .find(|r| {
            r.argv == "git commit -F -"
                && r.output
                    .as_deref()
                    .is_some_and(|o| o.contains("zz answer marker"))
        })
        .expect("the commit's record has git's answer");
    let answer = record.output.clone().unwrap();
    assert!(answer.starts_with("[main "), "{answer}");

    let lines = theme::command_lines(&Palette::DARK, &record);
    assert_eq!(lines.len(), 2, "the command, then its answer");
    let second: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(second.contains("zz answer marker"), "{second}");
}
