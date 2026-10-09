//! Layouts.

use crate::support::{buffer, col_of, render, stats, text, view};
use ferrit::theme::palette::Palette;
use ratatui::style::Color;

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
        .position(|l| l.contains("commits on main    authors"))
        .unwrap();
    let values = lines[labels - 1];
    for (value, label) in [
        ("423", "commits on main"),
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
