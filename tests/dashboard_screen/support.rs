//! Hand-built data and render helpers shared by the dashboard screen tests.

use ferrit::app::screens::dashboard::{self, Chrome, View};
use ferrit::components::ui::chart_palette::{ChartMode, ChartPalette};
use ferrit::domain::git::stats::branches::{BranchHealth, TagSince, VsMain};
use ferrit::domain::git::stats::kind::Kind;
use ferrit::domain::git::stats::series::{Bucket, Granularity};
use ferrit::domain::git::stats::share::Share;
use ferrit::domain::git::stats::{
    AuthorStat, FileStat, HotFiles, KindStat, Lines, RepoStats, Totals, Window, WorkState,
};
use ferrit::theme::palette::Palette;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

pub(crate) const NOW: i64 = 1_790_683_200;
pub(crate) const DAY: i64 = 86_400;

pub(crate) fn author(name: &str, commits: usize) -> AuthorStat {
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

pub(crate) fn branch(
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
pub(crate) fn stats() -> RepoStats {
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

pub(crate) fn view(stats: Option<&RepoStats>) -> View<'_> {
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
        chrome: Chrome::Framed,
        now: NOW,
    }
}

pub(crate) fn buffer(view: &View<'_>, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            dashboard::draw(f, f.area(), view);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

pub(crate) fn text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn render(view: &View<'_>, width: u16, height: u16) -> String {
    text(&buffer(view, width, height))
}

/// The cell column of `pat` in `line` (a rendered row: bytes are not cells).
pub(crate) fn col_of(line: &str, pat: &str) -> u16 {
    u16::try_from(line[..line.find(pat).unwrap()].chars().count()).unwrap()
}

pub(crate) fn is_braille(c: char) -> bool {
    ('\u{2800}'..='\u{28FF}').contains(&c)
}
