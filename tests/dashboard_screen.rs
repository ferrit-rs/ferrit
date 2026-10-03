#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    clippy::print_stdout,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The dashboard screen (`docs/PLAN_13_DASHBOARD.md`, D3b) rendered into a
//! `TestBackend`. Layout, colour and edge-case tests use hand-built `RepoStats`
//! at a fixed "now" (no git, no clock); a few tests go through `App` on fixture
//! repositories dated relative to the real clock, so they assert on labels and
//! figure shapes, never on relative-time text.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use ferrit::app::screens::dashboard::{self, View};
use ferrit::app::{App, screens as ui};
use ferrit::components::ui::chart_palette::{ChartMode, ChartPalette};
use ferrit::components::ui::palette::Palette;
use ferrit::domain::git::Repo;
use ferrit::domain::git::stats::branches::{BranchHealth, TagSince, VsMain};
use ferrit::domain::git::stats::kind::Kind;
use ferrit::domain::git::stats::series::{Bucket, Granularity};
use ferrit::domain::git::stats::share::Share;
use ferrit::domain::git::stats::{
    AuthorStat, FileStat, HotFiles, KindStat, Lines, RepoStats, StatsOptions, Totals, Window,
    WorkState,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::Color;

const NOW: i64 = 1_790_683_200;
const DAY: i64 = 86_400;

// ---------------------------------------------------------------- synthetic

fn author(name: &str, commits: usize) -> AuthorStat {
    let email = format!("{}@example.com", name.to_lowercase().replace(' ', "."));
    AuthorStat {
        name: name.to_owned(),
        emails: vec![email.clone()],
        email,
        commits,
        added: Some(0),
        removed: Some(0),
        last_commit: NOW - DAY,
    }
}

fn branch(
    name: &str,
    current: bool,
    ago_days: i64,
    vs: Option<(usize, usize, bool)>,
) -> BranchHealth {
    BranchHealth {
        name: name.to_owned(),
        current,
        tip_time: NOW - ago_days * DAY,
        vs_main: vs.map(|(ahead, behind, merged)| VsMain {
            ahead,
            behind,
            merged,
        }),
        stale: !current && ago_days > 60,
    }
}

/// Ferrit-shaped numbers: 423 commits, 8 authors, 12 branches, 13 weekly buckets.
fn stats() -> RepoStats {
    let hot = |path: &str, n: u64| FileStat {
        path: path.to_owned(),
        share: Share::of(n, 423),
        added: n * 10,
        removed: n,
    };
    let mut branches = vec![branch("main", true, 0, Some((0, 0, false)))];
    for i in 0..8_usize {
        branches.push(branch(
            &format!("feat/work_{i}"),
            false,
            3 + i64::try_from(i).unwrap(),
            Some((4 + 2 * i, 9, false)),
        ));
    }
    branches.push(branch("fix/old_idea", false, 130, Some((3, 200, false))));
    branches.push(branch("release/0.6", false, 20, Some((0, 20, true))));
    branches.push(branch("release/0.5", false, 90, Some((0, 90, true))));
    branches.push(branch("done", false, 12, Some((0, 12, true))));
    RepoStats {
        window: Window::Days90,
        totals: Totals {
            commits: 423,
            authors: 7,
            local_branches: 13,
            remote_branches: 3,
            remotes: 1,
            tags: 5,
            stashes: 0,
            first_commit: Some(NOW - 200 * DAY),
            last_commit: Some(NOW - 3600),
            lines: Some(Lines {
                added: 87_500,
                removed: 16_100,
            }),
        },
        series: (0..13_usize)
            .map(|i| Bucket {
                start: NOW - i64::try_from(13 - i).unwrap() * 7 * DAY,
                commits: 5 + (i * 7) % 11 * 4,
            })
            .collect(),
        granularity: Granularity::Week,
        daily: (0..182_usize)
            .map(|i| Bucket {
                start: NOW - i64::try_from(181 - i).unwrap() * DAY,
                commits: (i * 7) % 5,
            })
            .collect(),
        authors: [
            ("Richard Lavoura", 200),
            ("Max Wells", 100),
            ("Ola Nordmann", 50),
            ("Ana Silva", 30),
            ("Li Wei", 20),
            ("Zed Seventh", 15),
            ("Eve Eighth", 8),
        ]
        .iter()
        .map(|&(n, c)| author(n, c))
        .collect(),
        kinds: [
            (Kind::Feat, 124),
            (Kind::Docs, 103),
            (Kind::Chore, 60),
            (Kind::Fix, 54),
            (Kind::Other, 47),
            (Kind::Test, 20),
            (Kind::Refactor, 10),
            (Kind::Perf, 5),
        ]
        .iter()
        .map(|&(kind, commits)| KindStat { kind, commits })
        .collect(),
        hot_files: Some(HotFiles {
            files: vec![
                hot("src/app.rs", 85),
                hot("src/ui.rs", 84),
                hot(
                    "src/app/screens/dashboard/a_very_long_module_name/mod.rs",
                    60,
                ),
                hot("README.md", 30),
            ],
            hidden: vec!["CHANGELOG.md".to_owned(), "Cargo.lock".to_owned()],
            gone: 0,
            commits: 423,
        }),
        branches,
        main_branch: Some("main".to_owned()),
        work: WorkState {
            changed: 2,
            stashes: 1,
            upstream: Some("origin/main".to_owned()),
            ahead: 1,
            ..WorkState::default()
        },
        since_tag: Some(TagSince {
            name: "v0.7.0".to_owned(),
            commits: 35,
        }),
        shallow: false,
        sampled: false,
    }
}

fn view(stats: Option<&RepoStats>) -> View<'_> {
    View {
        stats,
        repo: "ferrit",
        branch: "main",
        colors: ChartPalette::for_palette(&Palette::DARK),
        mode: ChartMode::Braille,
        show_counts: false,
        computing: false,
        churn_pending: false,
        error: None,
        scroll: 0,
        now: NOW,
    }
}

fn buffer(view: &View<'_>, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            dashboard::draw(f, f.area(), view);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render(view: &View<'_>, width: u16, height: u16) -> String {
    text(&buffer(view, width, height))
}

/// The cell column of `pat` in `line` (a rendered row: bytes are not cells).
fn col_of(line: &str, pat: &str) -> u16 {
    u16::try_from(line[..line.find(pat).unwrap()].chars().count()).unwrap()
}

fn is_braille(c: char) -> bool {
    ('\u{2800}'..='\u{28FF}').contains(&c)
}

// ------------------------------------------------------------------- layouts

#[test]
fn wide_layout_has_every_section_and_the_header_and_progress_lines() {
    let s = stats();
    let out = render(&view(Some(&s)), 120, 42);
    println!("{out}");
    for want in [
        "ferrit · main",
        "Dashboard",
        "window: 90 days (t)",
        "Activity  commits per week",
        "13 weeks of history · peak ",
        "What was done",
        "Commits per day  23 weeks",
        "Contributors",
        "Hot files  share of commits touching",
        "Branches",
        "2 changed · 1 stash · main 1 commit ahead of origin/main",
    ] {
        assert!(out.contains(want), "{want:?} missing in\n{out}");
    }
    // Two columns: the first title row holds Activity and What was done.
    let row = out.lines().find(|l| l.contains("Activity")).unwrap();
    assert!(row.contains("What was done"), "{row}");
    assert!(
        out.lines()
            .any(|l| l.contains("Commits per day") && l.contains("Contributors"))
    );
}

/// No box-drawing junction and no vertical rule inside the page: the border is
/// the only frame, sections are a bold title over a thin rule.
fn assert_no_inner_box(out: &str, width: usize) {
    let rows: Vec<Vec<char>> = out.lines().map(|l| l.chars().collect()).collect();
    let left = rows[0].iter().position(|&c| c == '╭').unwrap();
    let right = rows[0].iter().position(|&c| c == '╮').unwrap();
    assert!(right < width);
    for (y, row) in rows.iter().enumerate() {
        if !row.contains(&'│') && !row.contains(&'╭') {
            continue;
        }
        for (x, &c) in row.iter().enumerate() {
            assert!(
                !matches!(c, '├' | '┬' | '┼' | '┴' | '┤' | '┌' | '┐' | '└' | '┘'),
                "junction {c} at ({x},{y})\n{out}"
            );
            if c == '│' {
                assert!(x == left || x == right, "inner vertical rule at ({x},{y})");
            }
        }
    }
}

#[test]
fn there_are_no_inner_boxes_only_the_outer_border() {
    let s = stats();
    for (w, h) in [(120, 60), (110, 60), (80, 120), (200, 60)] {
        let out = render(&view(Some(&s)), w, h);
        assert_no_inner_box(&out, usize::from(w));
    }
}

#[test]
fn the_border_is_one_rounded_line_in_the_dim_idle_colour() {
    let s = stats();
    let buf = buffer(&view(Some(&s)), 120, 60);
    let corner = &buf[(5, 0)];
    assert_eq!(corner.symbol(), "╭");
    assert_eq!(corner.fg, Palette::DARK.idle);
    assert!(corner.modifier.contains(ratatui::style::Modifier::DIM));
    // The rule under a section title has the same recessive style.
    let out = text(&buf);
    let row = out.lines().position(|l| l.contains("Activity")).unwrap() + 1;
    let rule = &buf[(10, u16::try_from(row).unwrap())];
    assert_eq!(rule.symbol(), "─");
    assert_eq!(rule.fg, Palette::DARK.idle);
    assert!(rule.modifier.contains(ratatui::style::Modifier::DIM));
}

#[test]
fn the_page_is_capped_at_110_columns_and_centred() {
    let s = stats();
    for width in [200_u16, 111, 150] {
        let out = render(&view(Some(&s)), width, 60);
        let first = out.lines().next().unwrap();
        let margin = (usize::from(width) - 110) / 2;
        assert_eq!(
            first.chars().position(|c| c == '╭'),
            Some(margin),
            "{width}"
        );
        assert_eq!(
            first.chars().position(|c| c == '╮'),
            Some(margin + 109),
            "{width}"
        );
        assert!(first.chars().take(margin).all(|c| c == ' '));
        let last = out.lines().find(|l| l.contains('╰')).unwrap();
        assert_eq!(last.chars().position(|c| c == '╰'), Some(margin));
    }
    // At exactly 110 there is no margin, and under it the page is the terminal.
    let out = render(&view(Some(&s)), 110, 60);
    assert_eq!(out.lines().next().unwrap().chars().next(), Some('╭'));
    let out = render(&view(Some(&s)), 90, 120);
    assert_eq!(out.lines().next().unwrap().chars().last(), Some('╮'));
}

#[test]
fn the_two_columns_are_four_columns_apart_with_no_rule_between() {
    let s = stats();
    let out = render(&view(Some(&s)), 120, 60);
    let rule = out
        .lines()
        .skip_while(|l| !l.contains("Activity"))
        .nth(1)
        .unwrap();
    let cells: Vec<char> = rule.chars().collect();
    let dashes: Vec<usize> = cells
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c == '─')
        .map(|(x, _)| x)
        .collect();
    let left_end = dashes
        .iter()
        .copied()
        .find(|&x| cells[x + 1] == ' ')
        .unwrap();
    let right_start = dashes.iter().copied().find(|&x| x > left_end).unwrap();
    assert_eq!(right_start - left_end - 1, 4, "{rule}");
    assert_no_inner_box(&out, 120);
}

#[test]
fn the_tiles_show_the_value_above_its_label() {
    let s = stats();
    let out = render(&view(Some(&s)), 120, 60);
    let lines: Vec<&str> = out.lines().collect();
    let labels = lines
        .iter()
        .position(|l| l.contains("commits    authors"))
        .unwrap();
    let values = lines[labels - 1];
    for (value, label) in [
        ("423", "commits"),
        ("7", "authors"),
        ("13 (+3 remote)", "branches"),
        ("5", "tags"),
        ("35", "since v0.7.0"),
    ] {
        let x = col_of(lines[labels], label);
        assert_eq!(col_of(values, value), x, "{value} above {label}");
    }
    // No boxes and no sentence line.
    assert!(!out.contains("Commits 423 ·"), "{out}");
}

#[test]
fn the_commit_count_is_the_hero_tile_and_the_labels_are_dim() {
    let s = stats();
    let buf = buffer(&view(Some(&s)), 120, 60);
    let out = text(&buf);
    let vy = out.lines().position(|l| l.contains("423")).unwrap();
    let vy = u16::try_from(vy).unwrap();
    let x = col_of(out.lines().nth(usize::from(vy)).unwrap(), "423");
    let hero = &buf[(x, vy)];
    assert_eq!(hero.fg, Palette::DARK.focus);
    assert!(hero.modifier.contains(ratatui::style::Modifier::BOLD));
    let other = &buf[(
        col_of(out.lines().nth(usize::from(vy)).unwrap(), "13 (+3"),
        vy,
    )];
    assert_eq!(other.fg, Color::Reset, "the other values are primary ink");
    assert!(other.modifier.contains(ratatui::style::Modifier::BOLD));
    let label = &buf[(x, vy + 1)];
    assert!(label.modifier.contains(ratatui::style::Modifier::DIM));
}

#[test]
fn the_bars_are_thin_and_the_donut_and_the_heat_map_have_no_block_glyphs() {
    let s = stats();
    let out = render(&view(Some(&s)), 120, 60);
    assert!(out.contains('━') && out.contains('─'));
    assert!(!out.contains('█') && !out.contains('░'), "{out}");
}

#[test]
fn percentages_come_first_and_counts_follow_in_brackets() {
    let s = stats();
    let out = render(&view(Some(&s)), 120, 42);
    // kinds: feat 124 of 423 is 29 %; hot files: 85 of 423 is 20 %.
    assert!(out.contains("29 %  (124)"), "{out}");
    assert!(out.contains("20 %  (85)"), "{out}");
    // contributors: 200 of 423 is 47 %.
    assert!(out.contains("47 %  (200)"), "{out}");
    // lines: 87.5k of 103.6k is 84 %.
    assert!(
        out.contains("+84 %  (87.5k)") && out.contains("−16 %  (16.1k)"),
        "{out}"
    );
    let mut counts = view(Some(&s));
    counts.show_counts = true;
    let out = render(&counts, 120, 42);
    assert!(
        out.contains("124  (29 %)") && out.contains("+87.5k  (84 %)"),
        "{out}"
    );
    assert!(!out.contains("29 %  (124)"));
}

#[test]
fn the_legend_has_a_marker_per_kind_and_folds_the_small_ones_into_others() {
    let s = stats();
    let out = render(&view(Some(&s)), 120, 42);
    for want in ["● feat", "■ fix", "▲ docs", "◆ test", "○ others"] {
        assert!(out.contains(want), "{want:?} missing in\n{out}");
    }
    // refactor is 2 % of 423: folded, and so are perf and chore into others.
    assert!(!out.contains("refactor") && !out.contains("chore"));
}

#[test]
fn narrow_layout_stacks_the_sections_and_scrolls() {
    let s = stats();
    let tall = render(&view(Some(&s)), 80, 120);
    for want in [
        "Activity",
        "What was done",
        "Commits per day",
        "Contributors",
        "Hot files",
        "Branches",
        "main 1 commit ahead",
    ] {
        assert!(tall.contains(want), "{want:?} missing");
    }
    assert_no_inner_box(&tall, 80);
    let mut v = view(Some(&s));
    let top = render(&v, 80, 20);
    assert!(top.contains("Activity") && !top.contains("main 1 commit ahead"));
    v.scroll = 5;
    let scrolled = render(&v, 80, 20);
    assert_ne!(top, scrolled, "scrolling moves the page");
    v.scroll = usize::MAX;
    let end = render(&v, 80, 20);
    assert!(end.contains("main 1 commit ahead"), "{end}");
    v.scroll = 100_000;
    assert_eq!(end, render(&v, 80, 20), "the renderer clamps the scroll");
}

#[test]
fn a_very_narrow_terminal_shows_the_totals_and_the_work_in_progress() {
    let s = stats();
    let out = render(&view(Some(&s)), 50, 20);
    assert!(out.contains("Commits 423"), "{out}");
    assert!(out.contains("2 changed"));
    assert!(out.contains("widen the terminal for charts"));
    assert!(!out.contains("Activity") && !out.contains("Contributors"));
    for line in out.lines() {
        assert!(line.chars().count() <= 50);
    }
}

// ------------------------------------------------------------ section detail

#[test]
fn contributors_are_share_bars_and_more_than_six_fold_into_others() {
    let mut s = stats();
    s.authors.push(author("Nine Ninth", 5));
    s.totals.authors = 9;
    let out = render(&view(Some(&s)), 120, 42);
    for name in [
        "Richard Lavoura",
        "Max Wells",
        "Ola Nordmann",
        "Ana Silva",
        "Li Wei",
    ] {
        assert!(out.contains(name), "{name}");
    }
    assert!(!out.contains("Zed Seventh") && !out.contains("Nine Ninth"));
    assert!(out.contains("others"));
    assert!(out.contains('━') && out.contains('─'));
    // Shares are of the whole: 200 of 428 is 47 %, others is 28 of 428.
    assert!(out.contains("47 %  (200)"), "{out}");
    assert!(out.contains("6 %  (28)"), "{out}");
}

#[test]
fn every_contributor_bar_wears_the_one_accent_and_the_text_stays_ink() {
    let s = stats();
    let buf = buffer(&view(Some(&s)), 120, 60);
    let out = text(&buf);
    let mut bar_colours = std::collections::HashSet::new();
    for name in [
        "Richard Lavoura",
        "Max Wells",
        "Ola Nordmann",
        "Ana Silva",
        "Li Wei",
        "others",
    ] {
        let y = out
            .lines()
            .position(|l| l.contains(name) && l.contains('━'))
            .unwrap();
        let y = u16::try_from(y).unwrap();
        let row = out.lines().nth(usize::from(y)).unwrap();
        let name_cell = &buf[(col_of(row, name), y)];
        assert_ne!(name_cell.fg, Palette::DARK.focus, "{name} is ink");
        let bar = &buf[(col_of(row, "━"), y)];
        assert_eq!(bar.symbol(), "━");
        bar_colours.insert(bar.fg);
        let pct = &buf[(col_of(row, " %") - 1, y)];
        assert_ne!(pct.fg, Palette::DARK.focus, "the percentage is ink");
    }
    assert_eq!(
        bar_colours,
        std::collections::HashSet::from([Palette::DARK.focus]),
        "one series, one colour"
    );
}

#[test]
fn an_author_with_several_emails_says_so_after_the_name() {
    let mut s = stats();
    s.authors[0].emails = vec!["a@x.org".into(), "b@x.org".into(), "c@x.org".into()];
    let out = render(&view(Some(&s)), 120, 60);
    let row = out.lines().find(|l| l.contains("Richard Lavoura")).unwrap();
    assert!(row.contains("Richard Lavoura (3 emails)"), "{row}");
    let other = out.lines().find(|l| l.contains("Max Wells")).unwrap();
    assert!(!other.contains("emails"), "{other}");
}

#[test]
fn the_hot_files_footer_counts_the_files_gone_from_the_tree() {
    let mut s = stats();
    s.hot_files.as_mut().unwrap().gone = 3;
    // Both notes do not fit a 51-column column: two dim lines.
    let out = render(&view(Some(&s)), 120, 60);
    assert!(
        out.contains("2 files hidden (CHANGELOG.md, Cargo.lock)"),
        "{out}"
    );
    assert!(out.contains("3 files no longer in the tree"), "{out}");
    let hidden = out
        .lines()
        .position(|l| l.contains("files hidden"))
        .unwrap();
    assert_eq!(
        out.lines()
            .position(|l| l.contains("no longer in the tree"))
            .unwrap(),
        hidden + 1
    );
    // In one 76-column column they share a line.
    let out = render(&view(Some(&s)), 80, 120);
    assert!(
        out.contains("2 files hidden (CHANGELOG.md, Cargo.lock) · 3 files no longer in the tree"),
        "{out}"
    );
    // Nothing hidden: only the gone note; none of either: no footer.
    s.hot_files.as_mut().unwrap().hidden.clear();
    let out = render(&view(Some(&s)), 80, 120);
    assert!(out.contains("3 files no longer in the tree") && !out.contains("hidden"));
    s.hot_files.as_mut().unwrap().gone = 1;
    let out = render(&view(Some(&s)), 80, 120);
    assert!(out.contains("1 file no longer in the tree"), "{out}");
}

#[test]
fn one_author_and_one_kind_are_a_hundred_percent() {
    let mut s = stats();
    s.authors = vec![author("Solo Dev", 423)];
    s.totals.authors = 1;
    s.kinds = vec![KindStat {
        kind: Kind::Feat,
        commits: 423,
    }];
    let out = render(&view(Some(&s)), 120, 42);
    assert!(
        out.contains("Solo Dev") && out.contains("100 %  (423)"),
        "{out}"
    );
    assert!(out.contains("● feat"));
    assert!(!out.contains("○ others"));
}

#[test]
fn a_repository_without_conventional_prefixes_says_so() {
    let mut s = stats();
    s.kinds = vec![KindStat {
        kind: Kind::Other,
        commits: 423,
    }];
    let out = render(&view(Some(&s)), 120, 42);
    assert!(out.contains("no conventional prefixes"), "{out}");
}

#[test]
fn hot_files_are_shares_of_commits_with_a_hidden_footer_and_paths_cut_in_the_middle() {
    let s = stats();
    let out = render(&view(Some(&s)), 120, 42);
    assert!(
        out.contains("2 files hidden (CHANGELOG.md, Cargo.lock)"),
        "{out}"
    );
    let long = out.lines().find(|l| l.contains("mod.rs")).unwrap();
    assert!(long.contains('…') && long.contains("src/"), "{long}");
    assert!(out.contains("src/app.rs"));
}

#[test]
fn branches_summarise_then_list_eight_rows_and_the_rest_as_more() {
    let s = stats();
    let out = render(&view(Some(&s)), 120, 50);
    // 13 branches: current + 8 feat are active (9), 3 merged, 1 stale (fix/old_idea);
    // release/0.5 is merged and old, so it counts as stale too.
    assert!(out.contains("9 active · 2 merged · 2 stale"), "{out}");
    assert!(out.contains("+5 more"), "{out}");
    assert!(out.contains("↑4 ↓9"), "{out}");
    let main_row = out.lines().find(|l| l.contains("main ●")).unwrap();
    assert!(main_row.contains("↑0 ↓0"), "{main_row}");
    assert!(out.contains("3 d ago") && out.contains("just now"), "{out}");
}

#[test]
fn stale_branches_carry_the_word_and_the_warn_colour() {
    let mut s = stats();
    s.branches = vec![
        branch("main", true, 0, Some((0, 0, false))),
        branch("fix/old_idea", false, 130, Some((3, 200, false))),
        branch("done", false, 12, Some((0, 12, true))),
    ];
    let buf = buffer(&view(Some(&s)), 120, 42);
    let out = text(&buf);
    let row = out
        .lines()
        .position(|l| l.contains("fix/old_idea"))
        .unwrap();
    let line = out.lines().nth(row).unwrap();
    assert!(line.contains("stale"), "{line}");
    let y = u16::try_from(row).unwrap();
    let x = col_of(line, "stale");
    assert_eq!(
        buf[(x, y)].fg,
        Palette::DARK.warn,
        "the stale word is the alert colour"
    );
    // The merged one is gray, the current one is the accent in bold.
    let merged = out.lines().position(|l| l.contains("  done ")).unwrap();
    let merged_x = col_of(out.lines().nth(merged).unwrap(), "done");
    assert_eq!(
        buf[(merged_x, u16::try_from(merged).unwrap())].fg,
        Color::Gray
    );
    let main_row = out.lines().position(|l| l.contains("main ●")).unwrap();
    let mx = col_of(out.lines().nth(main_row).unwrap(), "main");
    let cell = &buf[(mx, u16::try_from(main_row).unwrap())];
    assert_eq!(cell.fg, Palette::DARK.focus);
    assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
}

#[test]
fn the_donut_legend_marker_has_the_colour_of_its_ring() {
    let s = stats();
    let buf = buffer(&view(Some(&s)), 120, 42);
    let colors = ChartPalette::for_palette(&Palette::DARK);
    let mut checked = 0;
    for (marker, want) in [
        ("●", colors.kind_color(Kind::Feat)),
        ("■", colors.kind_color(Kind::Fix)),
        ("▲", colors.kind_color(Kind::Docs)),
        ("◆", colors.kind_color(Kind::Test)),
        ("○", colors.kind_color(Kind::Other)),
    ] {
        let cell = (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .map(|p| &buf[p])
            .find(|c| c.symbol() == marker)
            .unwrap_or_else(|| panic!("no {marker} in the legend"));
        assert_eq!(cell.fg, want, "legend {marker}");
        let ring = (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .map(|p| &buf[p])
            .any(|c| c.symbol().chars().next().is_some_and(is_braille) && c.fg == want);
        assert!(ring, "the ring has cells of the colour of {marker}");
        checked += 1;
    }
    assert_eq!(checked, 5);
}

#[test]
fn the_blocks_fallback_draws_no_braille() {
    let s = stats();
    let mut v = view(Some(&s));
    let braille = render(&v, 120, 42);
    assert!(braille.chars().any(is_braille));
    v.mode = ChartMode::Blocks;
    let out = render(&v, 120, 42);
    assert!(!out.chars().any(is_braille), "{out}");
    assert!(
        out.chars().any(|c| ('▁'..='█').contains(&c)),
        "the sparkline"
    );
    assert!(
        out.contains("● feat"),
        "the donut became a bar with its legend"
    );
    assert!(out.contains("peak"));
}

#[test]
fn without_braille_the_donut_is_one_full_width_stacked_bar_above_its_legend() {
    let s = stats();
    let mut v = view(Some(&s));
    v.mode = ChartMode::Blocks;
    let out = render(&v, 100, 120);
    let lines: Vec<&str> = out.lines().collect();
    let title = lines
        .iter()
        .position(|l| l.contains("What was done"))
        .unwrap();
    let bar: String = lines[title + 2].chars().skip(2).take(96).collect();
    assert!(bar.chars().all(|c| c == '━'), "{bar}");
    assert!(lines[title + 3].contains("● feat"));
}

#[test]
fn the_line_chart_has_no_axis_box_one_end_dot_and_two_ticks() {
    let s = stats();
    let out = render(&view(Some(&s)), 80, 120);
    let lines: Vec<Vec<char>> = out.lines().map(|l| l.chars().collect()).collect();
    let title = lines
        .iter()
        .position(|l| l.iter().collect::<String>().contains("Activity"))
        .unwrap();
    let caption = lines
        .iter()
        .position(|l| l.iter().collect::<String>().contains("weeks of history"))
        .unwrap();
    let inside = |row: &Vec<char>| row.iter().skip(1).take(78).collect::<String>();
    let plot: Vec<String> = lines[title + 1..caption].iter().map(inside).collect();
    for row in &plot {
        assert!(
            !row.contains(['│', '└', '┘', '┌', '┐', '┤', '├', '┬', '┴', '┼']),
            "an axis box in {row:?}"
        );
    }
    let all = plot.join("\n");
    assert!(all.chars().any(is_braille), "{all}");
    assert_eq!(all.matches('●').count(), 1, "one end marker\n{all}");
    // Two ticks only: the peak on the first plot row, 0 on the last.
    let peak = s
        .series
        .iter()
        .map(|b| b.commits)
        .max()
        .unwrap()
        .to_string();
    assert!(
        plot.iter()
            .filter(|r| r.trim_start().starts_with(&peak))
            .count()
            >= 1
    );
    assert!(plot.last().unwrap().trim_start().starts_with('0'), "{all}");
    assert!(
        lines[caption]
            .iter()
            .collect::<String>()
            .contains(&format!("13 weeks of history · peak {peak} ("))
    );
}

#[test]
fn the_heat_map_is_one_glyph_in_five_colours_and_glyph_density_without_colour() {
    let s = stats();
    let buf = buffer(&view(Some(&s)), 120, 42);
    let out = text(&buf);
    let row = out.lines().find(|l| l.contains("Mon ")).unwrap();
    assert!(row.contains('■'), "{row}");
    assert!(!out.contains('░') && !out.contains('▒') && !out.contains('▓'));
    let fills: std::collections::HashSet<_> = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .map(|p| &buf[p])
        .filter(|c| c.symbol() == "■")
        .map(|c| (c.fg, c.modifier))
        .collect();
    assert!(fills.len() >= 4, "levels differ by colour: {fills:?}");
    assert!(out.contains("less ■ ■ ■ ■ ■ more"), "{out}");

    let mut plain = view(Some(&s));
    plain.colors.density = true;
    let out = render(&plain, 120, 42);
    let row = out.lines().find(|l| l.contains("Mon ")).unwrap();
    assert!(
        row.contains('░') || row.contains('▒') || row.contains('▓') || row.contains('█'),
        "{row}"
    );
    assert!(out.contains("less · ░ ▒ ▓ █ more"), "{out}");
}

// ----------------------------------------------------------------- edge cases

#[test]
fn while_computing_the_screen_says_so() {
    let mut v = view(None);
    v.computing = true;
    let out = render(&v, 120, 30);
    assert!(
        out.contains("computing…") && out.contains("Dashboard"),
        "{out}"
    );
    assert!(!out.contains("Activity"));
    let out = render(&v, 50, 12);
    assert!(out.contains("computing…"));
}

#[test]
fn a_stats_error_is_one_line() {
    let mut v = view(None);
    v.error = Some("could not walk the refs");
    let out = render(&v, 120, 30);
    assert!(
        out.contains("could not read the statistics: could not walk the refs"),
        "{out}"
    );
    // Next to stats, it is a notice under the totals and the charts stay.
    let s = stats();
    let mut v = view(Some(&s));
    v.error = Some("boom");
    let out = render(&v, 120, 42);
    assert!(out.contains("stats error: boom") && out.contains("Contributors"));
}

#[test]
fn churn_columns_compute_then_read_n_a_when_missing() {
    let mut s = stats();
    s.hot_files = None;
    s.totals.lines = None;
    let mut v = view(Some(&s));
    v.churn_pending = true;
    let out = render(&v, 120, 42);
    assert_eq!(
        out.matches("computing…").count(),
        2,
        "hot files and lines: {out}"
    );
    assert!(out.contains("Contributors"), "the rest still renders");
    v.churn_pending = false;
    let out = render(&v, 120, 42);
    assert!(!out.contains("computing…"));
    assert!(out.matches("n/a").count() >= 2, "{out}");
}

#[test]
fn an_empty_repository_says_no_commits_yet() {
    let mut s = stats();
    s.totals = Totals {
        commits: 0,
        authors: 0,
        local_branches: 0,
        remote_branches: 0,
        remotes: 0,
        tags: 0,
        stashes: 0,
        first_commit: None,
        last_commit: None,
        lines: None,
    };
    s.series.clear();
    s.daily.clear();
    s.authors.clear();
    s.kinds.clear();
    s.hot_files = None;
    s.branches.clear();
    s.since_tag = None;
    let out = render(&view(Some(&s)), 120, 30);
    assert!(
        out.contains("no commits yet") && out.contains("commits"),
        "{out}"
    );
    assert!(!out.contains("Activity") && out.contains("2 changed"));
}

#[test]
fn a_window_without_commits_keeps_the_totals_and_empties_the_charts() {
    let mut s = stats();
    s.window = Window::Days7;
    s.totals.commits = 0;
    s.totals.authors = 0;
    s.series.clear();
    s.authors.clear();
    s.kinds.clear();
    s.hot_files = Some(HotFiles {
        files: vec![],
        hidden: vec![],
        gone: 0,
        commits: 0,
    });
    s.totals.lines = Some(Lines::default());
    let out = render(&view(Some(&s)), 120, 42);
    assert!(out.contains("window: 7 days (t)"));
    assert!(
        out.matches("no commits in this window").count() >= 4,
        "{out}"
    );
    assert!(out.contains("13 (+3 remote)"), "totals unchanged");
}

#[test]
fn the_shallow_and_sampled_banners() {
    let mut s = stats();
    s.shallow = true;
    s.sampled = true;
    let out = render(&view(Some(&s)), 120, 44);
    assert!(
        out.contains("shallow: history before 2026-03-13 is not available"),
        "{out}"
    );
    assert!(
        out.contains("sampled: newest 20 000 commits read, lines from the newest 5 000"),
        "{out}"
    );
}

#[test]
fn fewer_than_two_buckets_is_text_not_a_chart() {
    let mut s = stats();
    s.series = vec![Bucket {
        start: NOW - 3 * DAY,
        commits: 3,
    }];
    s.totals.commits = 3;
    let out = render(&view(Some(&s)), 120, 42);
    assert!(out.contains("3 commits this week"), "{out}");
}

#[test]
fn the_activity_caption_follows_the_granularity() {
    let mut s = stats();
    s.granularity = Granularity::Day;
    let out = render(&view(Some(&s)), 120, 42);
    assert!(
        out.contains("Activity  commits per day") && out.contains("13 days of history · peak "),
        "{out}"
    );
    s.granularity = Granularity::Month;
    let out = render(&view(Some(&s)), 120, 42);
    assert!(
        out.contains("commits per month") && out.contains("13 months of history · peak "),
        "{out}"
    );
}

#[test]
fn no_remote_hides_the_remote_count_and_a_small_whole_shows_counts() {
    let mut s = stats();
    s.totals.remote_branches = 0;
    s.totals.remotes = 0;
    s.since_tag = None;
    s.totals.commits = 12;
    s.kinds = vec![
        KindStat {
            kind: Kind::Feat,
            commits: 7,
        },
        KindStat {
            kind: Kind::Fix,
            commits: 5,
        },
    ];
    s.authors = vec![author("Solo Dev", 12)];
    s.hot_files = Some(HotFiles {
        files: vec![FileStat {
            path: "a.rs".to_owned(),
            share: Share::of(6, 12),
            added: 1,
            removed: 0,
        }],
        hidden: vec![],
        gone: 0,
        commits: 12,
    });
    let out = render(&view(Some(&s)), 120, 42);
    assert!(out.contains("branches") && !out.contains("remote"), "{out}");
    assert!(out.contains("Hot files  commits touching"));
    assert!(!out.contains("since"));
    // 12 commits: counts, no percentages, in the kinds legend, the bars and the branches.
    let legend = out.lines().find(|l| l.contains("● feat")).unwrap();
    assert!(legend.contains(" 7") && !legend.contains('%'), "{legend}");
    let solo = out.lines().find(|l| l.contains("Solo Dev")).unwrap();
    assert!(solo.contains("12") && !solo.contains('%'), "{solo}");
}

// ------------------------------------------------------------ through the App

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
    assert!(panes.contains("Stash") && !panes.contains("Dashboard"));
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
    assert!(out.contains("Stash") && !out.contains("Dashboard"));
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
        out.contains("Dashboard") && out.contains("[1] Status"),
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
