//! Section detail.

use crate::support::{author, branch, buffer, col_of, is_braille, render, stats, text, view};
use ferrit::git::stats::KindStat;
use ferrit::git::stats::kind::Kind;
use ferrit::theme::palette::Palette;
use ferrit::ui::widgets::chart_palette::{ChartMode, ChartPalette};
use ratatui::style::Color;

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
