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

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use ferrit::tui::App;
use ferrit::tui::components::panes::nav::Pane;
use ferrit::tui::draw;
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
    terminal.draw(|f| draw::draw(f, app)).unwrap();
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
}

/// Done when: with no stash entries and the Stash pane focused, the right pane says
/// "No stash entries", as lazygit's does (repo-tour step 8).
#[test]
fn an_empty_stash_pane_says_no_stash_entries() {
    let repo = Repo::new("empty-stash");
    repo.commit("a.txt", "one\n", "init");
    let mut app = repo.app();
    key(&mut app, '5');
    let out = frame(&mut app);
    assert!(out.contains("No stash entries"), "{out}");
}

/// Done when: with a stash selected, the right pane starts with the stash's own
/// subject, then its stat, then the patch (stash step 6).
#[test]
fn a_stash_shows_its_subject_and_stat_above_the_patch() {
    let repo = Repo::new("stash-header");
    repo.commit("a.txt", "one\n", "init");
    repo.write("a.txt", "one\ntwo\n");
    repo.git(&["stash", "push", "-m", "wip: demo"]);
    let mut app = repo.app();
    key(&mut app, '5');
    std::thread::sleep(std::time::Duration::from_millis(300));
    app.refresh();
    let out = frame(&mut app);
    let at = |needle: &str| {
        out.find(needle)
            .unwrap_or_else(|| panic!("{needle}: {out}"))
    };
    let (head, stat, patch) = (
        at("stash@{0}: On main: wip: demo"),
        at("1 file changed, 1 insertion(+)"),
        at("diff --git"),
    );
    assert!(head < stat && stat < patch, "{out}");
}

/// The foreground colour of the first cell of `text` where it appears on the frame.
fn colour_of(app: &mut App, text: &str) -> Option<ratatui::style::Color> {
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal.draw(|f| draw::draw(f, app)).unwrap();
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

/// Done when: the Files pane colours by staging state as lazygit does: the name of a
/// fully staged file is green with its letter, an untracked or unstaged name stays
/// default, a directory's arrow and name are green when all under it is staged, yellow
/// when part of it is, default when none is, and the selected row keeps them.
#[test]
fn files_colour_names_and_directories_by_staging_state() {
    use ratatui::style::Color;
    let repo = Repo::new("stage-colours");
    repo.commit("m.txt", "one\n", "init");
    repo.commit("w.txt", "one\n", "init w");
    repo.write("m.txt", "one\ntwo\n");
    repo.write("w.txt", "one\ntwo\n");
    repo.write("u.txt", "fresh\n");
    repo.write("zdone/a.txt", "a\n");
    repo.write("zdone/sub/b.txt", "b\n");
    repo.write("zpart/p1.txt", "p1\n");
    repo.write("zpart/p2.txt", "p2\n");
    repo.write("znone/n.txt", "n\n");
    repo.git(&["add", "m.txt", "zdone", "zpart/p1.txt"]);
    let mut app = repo.app();
    key(&mut app, '2');

    let colour = |app: &mut App, text: &str| colour_of(app, text).unwrap();
    assert_eq!(colour(&mut app, "M  m.txt"), Color::Green);
    assert_eq!(colour(&mut app, " m.txt"), Color::Green, "staged name");
    assert_eq!(colour(&mut app, " u.txt"), Color::Reset, "untracked name");
    assert_eq!(colour(&mut app, " w.txt"), Color::Reset, "unstaged name");
    assert_eq!(colour(&mut app, "\u{25bc} zdone"), Color::Green);
    assert_eq!(colour(&mut app, "zdone"), Color::Green);
    assert_eq!(colour(&mut app, "\u{25bc} sub"), Color::Green, "nested dir");
    assert_eq!(colour(&mut app, "\u{25bc} zpart"), Color::Yellow);
    assert_eq!(colour(&mut app, "zpart"), Color::Yellow);
    assert_ne!(colour(&mut app, "\u{25bc} znone"), Color::Green);
    assert_ne!(colour(&mut app, "\u{25bc} znone"), Color::Yellow);
    assert_eq!(colour(&mut app, "znone"), Color::Reset);
    // The root row is selected, and part of the tree is staged: yellow on the bar.
    assert_eq!(colour(&mut app, "\u{25bc} /"), Color::Yellow);

    // A file staged and modified again is only partly staged: its name is yellow.
    repo.write("m.txt", "one\ntwo\nthree\n");
    app.refresh();
    assert_eq!(
        colour(&mut app, " m.txt"),
        Color::Yellow,
        "MM name is yellow"
    );
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
    use ferrit::git::command_log;
    use ferrit::theme::palette::Palette;
    use ferrit::tui::row_lines;

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
            r.argv == "git commit -F -" && r.output.iter().any(|o| o.contains("zz answer marker"))
        })
        .expect("the commit's record has git's answer");
    assert!(
        record.output[0].starts_with("[main "),
        "{:?}",
        record.output
    );
    assert!(
        record.output.iter().any(|l| l.contains("1 file changed")),
        "the stat line too: {:?}",
        record.output
    );

    let lines = row_lines::status::command_lines(&Palette::DARK, &record);
    assert_eq!(
        lines.len(),
        1 + record.output.len(),
        "the command, then every answer line"
    );
    let second: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(second.contains("zz answer marker"), "{second}");
}

/// A repository whose `main` is published to a bare `origin`, with a tag on the first
/// commit and one more commit that is not pushed. Returns the short hashes.
fn published_repo(tag: &str) -> (Repo, String, String) {
    let repo = Repo::new(tag);
    repo.commit("a.txt", "one\n", "first");
    repo.git(&["tag", "v1"]);
    let origin = format!("{}-origin", repo.dir.display());
    let _ = fs::remove_dir_all(&origin);
    Command::new("git")
        .args(["init", "-q", "--bare", &origin])
        .output()
        .unwrap();
    repo.git(&["remote", "add", "origin", &origin]);
    repo.git(&["push", "-q", "-u", "origin", "main"]);
    repo.commit("a.txt", "one\ntwo\n", "second");
    let first = repo.git(&["rev-parse", "--short=7", "HEAD~1"]);
    let second = repo.git(&["rev-parse", "--short=7", "HEAD"]);
    (repo, first, second)
}

/// Done when: in the Commits list a tag shows before the subject (`v1 first`), and the hash
/// is red for a commit not pushed yet, green for one merged into origin/main (step 6, 11).
#[test]
fn the_commit_list_shows_tags_and_colours_hashes_by_push_state() {
    let (repo, first, second) = published_repo("list-state");
    let mut app = repo.app();
    key(&mut app, '4');
    key(&mut app, 'j'); // select the older commit so the newer row is drawn plain
    let out = frame(&mut app);
    assert!(out.contains("v1 first"), "a tag before the subject\n{out}");

    let unpushed = colour_of(&mut app, &second).unwrap();
    key(&mut app, 'k'); // now the older one is drawn plain
    let merged = colour_of(&mut app, &first).unwrap();
    assert_ne!(unpushed, merged, "unpushed and merged hashes differ");
    assert_eq!(
        unpushed,
        ratatui::style::Color::Red,
        "not pushed yet is red"
    );
}

/// Done when: the branch Log carries the decoration lazygit prints, `(HEAD -> main)` on
/// the tip and `(tag: v1, origin/main)` on the published commit (step 8).
#[test]
fn the_branch_log_shows_ref_decorations() {
    let (repo, _first, _second) = published_repo("log-decor");
    let mut app = repo.app();
    key(&mut app, '3');
    let log = frame(&mut app);
    assert!(log.contains("(HEAD -> main)"), "{log}");
    assert!(log.contains("(tag: v1, origin/main)"), "{log}");
}

/// Done when: a commit's Patch carries the same decoration on its `commit` line (step 12).
#[test]
fn a_commit_patch_shows_ref_decorations() {
    let (repo, _first, _second) = published_repo("patch-decor");
    let mut app = repo.app();
    key(&mut app, '4');
    let tip = frame(&mut app);
    assert!(tip.contains("(HEAD -> main)"), "{tip}");
    key(&mut app, 'j');
    let older = frame(&mut app);
    assert!(older.contains("(tag: v1, origin/main)"), "{older}");
}

/// Done when: a commit's Patch has lazygit's `---` line and per-file stat between the
/// message and the diff (step 12).
#[test]
fn a_commit_patch_has_the_stat_block_before_the_diff() {
    let (repo, _first, _second) = published_repo("stat");
    let mut app = repo.app();
    key(&mut app, '4');
    let out = frame(&mut app);
    let dashes = out.find("---").expect("the separator before the stat");
    let stat = out.find("a.txt | 1 +").expect("the per-file stat line");
    let summary = out.find("1 file changed").expect("the totals line");
    let diff = out.find("diff --git").expect("the diff");
    assert!(
        dashes < stat && stat < summary && summary < diff,
        "message, ---, stat, totals, then the diff\n{out}"
    );
}

/// Done when: `r` in Commits opens the reword popup, and the key bar shows `Reword: r`
/// (step 12); the older `w` still works.
#[test]
fn r_rewords_the_selected_commit_and_w_still_does() {
    let repo = Repo::new("reword-r");
    repo.commit("a.txt", "one\n", "first");
    repo.commit("a.txt", "one\ntwo\n", "second");
    let mut app = repo.app();
    key(&mut app, '4');
    assert!(frame(&mut app).contains("Reword: r"), "the bar shows r");
    for reword_key in ['r', 'w'] {
        key(&mut app, reword_key);
        assert!(
            app.commit_popup().is_some(),
            "{reword_key} opened the reword popup"
        );
        app.feed_key(KeyEvent::from(KeyCode::Esc));
    }
}

/// Done when: the new-branch prompt is titled `New branch name (branch is off of 'main')`,
/// naming the branch it starts from (step 7).
#[test]
fn the_new_branch_prompt_names_the_branch_it_starts_from() {
    let repo = Repo::new("prompt-base");
    repo.commit("a.txt", "one\n", "first");
    let mut app = repo.app();
    key(&mut app, '3');
    key(&mut app, 'n');
    let out = frame(&mut app);
    assert!(
        out.contains("New branch name (branch is off of 'main')"),
        "{out}"
    );
}

/// Done when: the branch list is the checked-out branch first, then the most recently
/// committed to, not alphabetical (step 8).
#[test]
fn branches_are_listed_current_first_then_most_recent_first() {
    let repo = Repo::new("branch-order");
    repo.commit("a.txt", "one\n", "first");
    // Commit times a day apart, so the order cannot be a tie on the second.
    let commit_at = |branch: &str, epoch: &str| {
        repo.git(&["checkout", "-q", "-b", branch]);
        repo.write(&format!("{branch}.txt"), "x\n");
        repo.git(&["add", "-A"]);
        let env_date = format!("{epoch} +0000");
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo.dir)
            .args(["commit", "-q", "-m", branch])
            .env("GIT_COMMITTER_DATE", &env_date)
            .env("GIT_AUTHOR_DATE", &env_date)
            .output()
            .unwrap();
        assert!(out.status.success());
        repo.git(&["checkout", "-q", "main"]);
    };
    commit_at("alpha-old", "1700000000");
    commit_at("zulu-new", "1700086400");
    commit_at("mid", "1700043200");
    let mut app = repo.app();
    key(&mut app, '3');
    let lines: Vec<String> = app.branch_lines().iter().map(ToString::to_string).collect();
    let names: Vec<&str> = lines
        .iter()
        .map(|l| l.split_whitespace().last().unwrap_or(""))
        .collect();
    assert_eq!(names, ["main", "zulu-new", "mid", "alpha-old"], "{lines:?}");
}

/// Done when: a history longer than 200 commits is counted in full, `1 of 260`, where it
/// stopped at `1 of 200` (step 6, 11).
#[test]
fn the_commit_counter_reads_the_real_total_beyond_200() {
    let repo = Repo::new("cap");
    let mut stream = String::new();
    for n in 0..260 {
        write!(
            stream,
            "commit refs/heads/main\ncommitter T <t@example.com> {} +0000\ndata 2\nc{}\n",
            1_700_000_000 + n,
            n % 10
        )
        .unwrap();
        if n == 0 {
            stream.push_str("M 100644 inline f.txt\ndata 2\nx\n");
        }
        stream.push('\n');
    }
    let mut child = Command::new("git")
        .arg("-C")
        .arg(&repo.dir)
        .args(["fast-import", "--quiet"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), stream.as_bytes()).unwrap();
    assert!(child.wait().unwrap().success());
    repo.git(&["reset", "-q", "--hard"]);
    let mut app = repo.app();
    key(&mut app, '4');
    let out = frame(&mut app);
    assert!(out.contains("1 of 260"), "{out}");
}

/// Done when: the `---` line of a commit's Patch is drawn plain, not in the removed-line red
/// (step 12).
#[test]
fn the_stat_separator_is_not_drawn_as_a_removed_line() {
    let repo = Repo::new("stat-sep");
    repo.commit("a.txt", "one\n", "first");
    let mut app = repo.app();
    key(&mut app, '4');
    let plain = colour_of(&mut app, "1 file changed").unwrap();
    // The first "---" on the frame is the separator: it comes before the diff's own.
    let separator = colour_of(&mut app, "---").unwrap();
    assert_eq!(separator, plain, "the separator has the text colour");
}

/// Done when: selecting a directory row shows the diff of the files under it, on the side
/// they are staged or unstaged, where the pane stayed empty (stage-directory flow).
#[test]
fn a_directory_row_shows_the_diff_of_what_is_under_it() {
    let repo = Repo::new("dir-diff");
    repo.commit("docs/a.md", "old\n", "init");
    repo.commit("outside.txt", "keep\n", "second");
    repo.write("docs/a.md", "old\nnew line under docs\n");
    repo.write("outside.txt", "keep\nnot under docs\n");
    let mut app = repo.app();
    key(&mut app, '2');
    let docs_row = (0..app.row_count(Pane::Files))
        .find(|&i| app.file_lines()[i].to_string().contains("docs"))
        .expect("a docs row");
    app.select(Pane::Files, docs_row);
    let out = frame(&mut app);
    assert!(out.contains("new line under docs"), "{out}");
    assert!(
        !out.contains("not under docs"),
        "only the directory's files\n{out}"
    );

    repo.git(&["add", "docs"]);
    app.refresh();
    app.select(Pane::Files, docs_row);
    let out = frame(&mut app);
    assert!(
        out.contains("Staged Changes"),
        "the staged side once staged\n{out}"
    );
    assert!(out.contains("new line under docs"), "{out}");
}

/// A directory row (the root row is selected at startup) must not split the right pane and
/// squeeze the side column to a few cells, whatever is staged under it.
#[test]
fn a_directory_row_keeps_the_side_column_at_full_width() {
    let repo = Repo::new("dir-width");
    repo.commit("docs/a.md", "old\n", "init");
    repo.write("docs/a.md", "old\nunstaged\n");
    repo.git(&["add", "docs/a.md"]);
    repo.write("docs/a.md", "old\nunstaged\nmore\n");
    repo.write("docs/b.md", "b\n");
    repo.git(&["add", "docs/b.md"]);
    let mut app = repo.app();
    key(&mut app, '2');
    app.select(Pane::Files, 0); // the root row
    let out = frame(&mut app);
    assert!(out.contains("[2] Files - Worktrees - Submodules"), "{out}");
}

/// Done when: on a branch with no upstream nothing is unpushed, so its newest commit is
/// yellow, not red (feature-workflow step 11); with an upstream the unpushed one stays red.
#[test]
fn a_branch_without_an_upstream_has_no_red_hashes() {
    let repo = Repo::new("no-upstream");
    repo.commit("a.txt", "one\n", "first");
    repo.commit("a.txt", "one\ntwo\n", "second");
    let second = repo.git(&["rev-parse", "--short=7", "HEAD"]);
    let mut app = repo.app();
    key(&mut app, '4');
    key(&mut app, 'j'); // select the older commit so the newer row is drawn plain
    assert_eq!(
        colour_of(&mut app, &second).unwrap(),
        ratatui::style::Color::Yellow,
        "no upstream: pushed, yellow"
    );
}

/// Done when: the branch Log prints a commit's message body under its subject, and the
/// Author line ends with the email as `Name <email>`; a subject-only commit has no body
/// rows (repo-tour step 3).
#[test]
fn the_branch_log_shows_the_body_and_the_author_email() {
    let repo = Repo::new("log-body");
    repo.commit("a.txt", "one\n", "plain subject");
    repo.commit(
        "a.txt",
        "one\ntwo\n",
        "with body\n\nfirst body line\nsecond body line",
    );
    let mut app = repo.app();
    key(&mut app, '3');
    let out = frame(&mut app);
    assert!(out.contains("Author: Test <test@example.com>"), "{out}");
    let subject = out.find("with body").expect("the subject");
    let first = out.find("first body line").expect("the body");
    let second = out.find("second body line").expect("the whole body");
    let older = out.find("plain subject").expect("the older subject");
    assert!(
        subject < first && first < second && second < older,
        "the body sits under its subject\n{out}"
    );
    let plain_tail = &out[older..];
    assert!(
        !plain_tail.contains("body line"),
        "no body under a bare subject\n{out}"
    );
}

/// Done when: a branch level with its upstream has a tick (✓) in the Status line and on
/// its Branches row, and none once it is ahead, or when it has no upstream (repo-tour
/// steps 1, 3).
#[test]
fn a_branch_level_with_its_upstream_has_a_tick() {
    let repo = Repo::new("tick");
    repo.commit("a.txt", "one\n", "init");
    let mut app = repo.app();
    assert!(
        !frame(&mut app).contains('\u{2713}'),
        "no upstream, no tick"
    );

    let remote = repo.dir.join("remote.git");
    repo.git(&["init", "-q", "--bare", remote.to_str().unwrap()]);
    repo.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    repo.git(&["push", "-q", "-u", "origin", "main"]);
    app.refresh();
    let status = app.status_lines()[0].to_string();
    assert!(status.ends_with("main \u{2713}"), "{status}");
    assert!(
        app.branch_lines()[0].to_string().contains('\u{2713}'),
        "the branch row"
    );

    repo.commit("a.txt", "one\ntwo\n", "second");
    app.refresh();
    let status = app.status_lines()[0].to_string();
    assert!(
        status.contains('\u{2191}') && !status.contains('\u{2713}'),
        "{status}"
    );
    assert!(!app.branch_lines()[0].to_string().contains('\u{2713}'));
}

/// Done when: with nothing changed, the Files key bar drops Stage, All, Discard and
/// Amend and keeps what works; with a change they are back (ui-mouse steps 1, 2).
#[test]
fn the_files_key_bar_on_a_clean_tree_offers_only_what_works() {
    let repo = Repo::new("clean-bar");
    repo.commit("a.txt", "one\n", "init");
    let mut app = repo.app();
    key(&mut app, '2');
    let out = frame(&mut app);
    assert!(out.contains("Commit: c"), "{out}");
    assert!(out.contains("Stash: s"), "{out}");
    for gone in ["Stage:", "All:", "Discard:", "Amend:"] {
        assert!(!out.contains(gone), "{gone} on a clean tree\n{out}");
    }

    repo.write("a.txt", "one\ntwo\n");
    app.refresh();
    let out = frame(&mut app);
    assert!(
        out.contains("Stage: <space>") && out.contains("Amend: A"),
        "{out}"
    );
}
