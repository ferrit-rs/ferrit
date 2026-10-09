//! Through the app.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use crate::support::{DAY, buffer, render, stats, text, view};
use ferrit::git::repo::Repo;
use ferrit::git::stats::{StatsOptions, Window};
use ferrit::tui::App;
use ferrit::tui::screens as ui;
use ferrit::tui::screens::dashboard::Chrome;
use ferrit::tui::widgets::chart_palette::ChartMode;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("ferrit-{tag}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
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

fn now_secs() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

fn git_as(dir: &Path, who: (&str, &str, i64), args: &[&str]) {
    let date = format!("{} +0000", who.2);
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", who.0)
        .env("GIT_AUTHOR_EMAIL", who.1)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_NAME", who.0)
        .env("GIT_COMMITTER_EMAIL", who.1)
        .env("GIT_COMMITTER_DATE", &date)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit(dir: &Path, who: (&str, &str), when: i64, message: &str, files: &[(&str, &str)]) {
    for (name, content) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let who = (who.0, who.1, when);
    git_as(dir, who, &["add", "-A"]);
    git_as(dir, who, &["commit", "-q", "-m", message]);
}

const RICHARD: (&str, &str) = ("Richard", "richard@example.com");
const MAX: (&str, &str) = ("Max", "max@example.com");
const OLA: (&str, &str) = ("Ola", "ola@example.com");

/// 24 commits over 24 days by three authors with every kind of prefix, a merged
/// branch, a branch ahead, a stale one, a tag, and an untracked file.
fn busy() -> TempDir {
    let tmp = TempDir::new("dash-busy");
    let dir = tmp.path();
    git_as(
        dir,
        ("F", "f@example.com", 0),
        &["init", "-q", "-b", "main"],
    );
    let now = now_secs();
    let subjects = [
        "feat: add",
        "fix: repair",
        "docs: explain",
        "test: cover",
        "refactor: tidy",
        "chore: tend",
    ];
    for i in 0..24_i64 {
        let who = [RICHARD, RICHARD, MAX, OLA][usize::try_from(i).unwrap() % 4];
        let subject = format!(
            "{} {i}",
            subjects[usize::try_from(i).unwrap() % subjects.len()]
        );
        let app = "line\n".repeat(usize::try_from(i + 1).unwrap());
        let files: Vec<(&str, String)> =
            vec![("src/app.rs", app), ("Cargo.lock", format!("l{i}\n"))];
        let files: Vec<(&str, &str)> = files.iter().map(|(n, c)| (*n, c.as_str())).collect();
        commit(dir, who, now - (25 - i) * DAY, &subject, &files);
        if i == 0 {
            git_as(dir, ("F", "f@example.com", now), &["branch", "old"]);
        }
        if i == 20 {
            git_as(dir, ("F", "f@example.com", now), &["branch", "done"]);
            git_as(dir, ("F", "f@example.com", now), &["tag", "v1"]);
        }
    }
    git_as(dir, ("F", "f@example.com", now), &["checkout", "-q", "old"]);
    commit(dir, OLA, now - 100 * DAY, "wip: stuff", &[("o.txt", "o\n")]);
    git_as(
        dir,
        ("F", "f@example.com", now),
        &["checkout", "-q", "-b", "feature", "main"],
    );
    commit(
        dir,
        MAX,
        now - 2 * DAY,
        "feat: on the branch",
        &[("f.txt", "f\n")],
    );
    git_as(
        dir,
        ("F", "f@example.com", now),
        &["checkout", "-q", "main"],
    );
    fs::write(dir.join("untracked.txt"), "u\n").unwrap();
    tmp
}

/// The frame after the drawer has finished sliding in (or out).
fn app_frame(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    for _ in 0..30 {
        app.advance_clock(std::time::Duration::from_millis(16));
        terminal.draw(|f| ui::draw(f, app)).unwrap();
    }
    terminal.backend().to_string()
}

fn key(app: &mut App, c: char) {
    app.feed_key(KeyEvent::from(KeyCode::Char(c)));
}

#[test]
fn the_dashboard_is_a_sheet_over_the_dimmed_panes_and_has_its_own_key_bar() {
    let tmp = busy();
    let mut app = App::open(tmp.path()).unwrap();
    let panes = app_frame(&mut app, 130, 50);
    assert!(panes.contains("Stash") && !panes.contains("Activity"));
    app.open_dashboard();
    assert!(app.dashboard_is_open());
    let out = app_frame(&mut app, 130, 50);
    println!("{out}");
    for want in [
        "Dashboard",
        "· main",
        "Activity",
        "What was done",
        "Contributors",
        "Hot files",
        "changed",
    ] {
        assert!(out.contains(want), "{want:?} missing in\n{out}");
    }
    assert!(
        out.contains("Stash") && out.contains("[1] Status"),
        "the panes stay in sight behind the sheet"
    );
    let last = out.lines().last().unwrap();
    assert!(
        last.contains("Back: esc | Window: t | Counts: n | Refresh: r | Help: ?"),
        "{last:?}"
    );
    let authors = out.lines().position(|l| l.contains("authors")).unwrap();
    assert!(
        out.lines().nth(authors - 1).unwrap().contains(" 3 ") && out.contains("1 untracked"),
        "{out}"
    );
    assert!(out.contains("1 file hidden (Cargo.lock)"), "{out}");
    assert!(
        out.contains("stale"),
        "the branch whose tip is 100 days old"
    );
    assert!(out.contains("main ●") && out.contains("feature") && out.contains("done"));
    // Back to the panes.
    key(&mut app, 'q');
    let out = app_frame(&mut app, 130, 50);
    assert!(out.contains("Stash") && !out.contains("Activity"));
}

#[test]
fn n_swaps_the_figures_to_counts_and_a_small_whole_shows_counts_anyway() {
    let tmp = busy();
    let mut app = App::open(tmp.path()).unwrap();
    app.open_dashboard();
    let out = app_frame(&mut app, 120, 40);
    // 26 commits in the window (24 + feature + ...): a whole of 20 or more.
    assert!(out.contains(" %  ("), "percent first: {out}");
    key(&mut app, 'n');
    let counted = app_frame(&mut app, 120, 40);
    assert!(
        counted.contains(" %)") && !counted.contains(" %  ("),
        "{counted}"
    );
    key(&mut app, 'n');
    assert_eq!(app_frame(&mut app, 120, 40), out);

    // A window with fewer than 20 commits shows counts only.
    for _ in 0..1 {
        key(&mut app, 't'); // 1 year
    }
    key(&mut app, 'T');
    key(&mut app, 'T'); // 30 days
    key(&mut app, 'T'); // 7 days
    let week = app_frame(&mut app, 120, 40);
    assert!(week.contains("window: 7 days (t)"), "{week}");
    assert!(
        !week.contains(" %"),
        "under 20 commits: counts, no percentages\n{week}"
    );
}

#[test]
fn the_help_overlay_still_draws_over_the_dashboard() {
    let tmp = busy();
    let mut app = App::open(tmp.path()).unwrap();
    app.open_dashboard();
    key(&mut app, '?');
    let out = app_frame(&mut app, 130, 50);
    assert!(out.contains("Close: esc/? | Scroll: j/k"), "{out}");
    assert!(
        out.contains("Dashb") && out.contains("[1] Status"),
        "the sheet and the panes stay under the overlay\n{out}"
    );
}

#[test]
fn scrolling_through_the_app_is_clamped_to_the_page() {
    let tmp = busy();
    let mut app = App::open(tmp.path()).unwrap();
    app.open_dashboard();
    let top = app_frame(&mut app, 80, 20);
    for _ in 0..5 {
        key(&mut app, 'j');
    }
    assert_ne!(app_frame(&mut app, 80, 20), top);
    app.feed_key(KeyEvent::from(KeyCode::End));
    let end = app_frame(&mut app, 80, 20);
    assert!(end.contains("changed"));
    // Past the end and back: one `k` moves up by one row, not by the overshoot.
    for _ in 0..50 {
        key(&mut app, 'j');
    }
    app_frame(&mut app, 80, 20);
    key(&mut app, 'k');
    assert_ne!(app_frame(&mut app, 80, 20), end);
}

#[test]
fn an_empty_repository_and_a_window_without_commits() {
    let empty = TempDir::new("dash-empty");
    git_as(
        empty.path(),
        ("F", "f@example.com", 0),
        &["init", "-q", "-b", "main"],
    );
    let mut app = App::open(empty.path()).unwrap();
    app.open_dashboard();
    let out = app_frame(&mut app, 120, 30);
    assert!(out.contains("no commits yet"), "{out}");

    let old = TempDir::new("dash-old");
    git_as(
        old.path(),
        ("F", "f@example.com", 0),
        &["init", "-q", "-b", "main"],
    );
    commit(
        old.path(),
        RICHARD,
        now_secs() - 400 * DAY,
        "feat: long ago",
        &[("a.txt", "a\n")],
    );
    let mut app = App::open(old.path()).unwrap();
    app.open_dashboard();
    let out = app_frame(&mut app, 120, 30);
    assert!(out.contains("no commits in this window"), "{out}");
    assert!(!out.contains("no commits yet"));
}

#[test]
fn a_shallow_clone_carries_the_banner() {
    let tmp = busy();
    let clone = TempDir::new("dash-shallow");
    let target = clone.path().join("c");
    let out = Command::new("git")
        .args(["clone", "-q", "--depth", "1", "--no-local"])
        .arg(format!("file://{}", tmp.path().display()))
        .arg(&target)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut app = App::open(&target).unwrap();
    app.open_dashboard();
    let frame = app_frame(&mut app, 120, 40);
    assert!(frame.contains("shallow: history before "), "{frame}");
}

#[test]
fn the_churn_columns_of_a_quick_pass_read_computing_then_the_full_pass_fills_them() {
    let tmp = busy();
    let repo = Repo::open(tmp.path()).unwrap();
    let quick = repo
        .stats_with(
            Window::Days90,
            &StatsOptions {
                churn: false,
                ..StatsOptions::default()
            },
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(quick.hot_files.is_none());
    let mut v = view(Some(&quick));
    v.now = now_secs();
    v.churn_pending = true;
    let out = render(&v, 120, 40);
    assert_eq!(out.matches("computing…").count(), 2, "{out}");
    let full = repo
        .stats_with(
            Window::Days90,
            &StatsOptions::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
    let mut v = view(Some(&full));
    v.now = now_secs();
    let out = render(&v, 120, 40);
    assert!(
        !out.contains("computing…") && out.contains("src/app.rs"),
        "{out}"
    );
    assert!(
        out.contains('+') && out.contains('−'),
        "lines added and removed: {out}"
    );
}

#[test]
#[ignore = "prints frames for a human to read: cargo test --test dashboard_screen show_frames -- --ignored --nocapture"]
fn show_frames() {
    let s = stats();
    let mut v = view(Some(&s));
    for (w, h, mode) in [
        (100, 42, ChartMode::Braille),
        (110, 60, ChartMode::Braille),
        (200, 42, ChartMode::Braille),
        (80, 30, ChartMode::Blocks),
        (50, 10, ChartMode::Braille),
    ] {
        v.mode = mode;
        println!("--- {w}x{h} {mode:?}\n{}", render(&v, w, h));
    }
}

#[test]
#[ignore = "prints real fixture frames for a human to read: cargo test --test dashboard_screen show_app_frames -- --ignored --nocapture"]
fn show_app_frames() {
    let tmp = busy();
    let mut app = App::open(tmp.path()).unwrap();
    app.open_dashboard();
    for (w, h) in [(100, 50), (200, 50)] {
        println!("--- through App {w}x{h}\n{}", app_frame(&mut app, w, h));
    }
}

#[test]
fn an_unframed_page_has_no_border_and_no_dashboard_title_and_starts_on_the_header() {
    let stats = stats();
    let framed = view(Some(&stats));
    let mut unframed = view(Some(&stats));
    unframed.chrome = Chrome::Bare;

    let with = buffer(&framed, 120, 60);
    let without = buffer(&unframed, 120, 60);
    let (with, without) = (text(&with), text(&without));
    assert!(with.contains('╭') && with.contains("Dashboard"), "{with}");
    assert!(
        !without.contains('╭') && !without.contains('╰'),
        "{without}"
    );
    assert!(
        !without.contains("Dashboard"),
        "the sheet's own title says it\n{without}"
    );
    assert!(without.contains("· main"), "the header is still there");
    // The first line is the header, not a blank row.
    assert!(without.lines().next().unwrap().contains("· main"));
}

#[test]
fn an_unframed_compact_page_is_unframed_too() {
    let stats = stats();
    let mut v = view(Some(&stats));
    v.chrome = Chrome::Bare;
    let out = render(&v, 40, 12);
    assert!(!out.contains('╭') && out.contains("· main"), "{out}");
}
