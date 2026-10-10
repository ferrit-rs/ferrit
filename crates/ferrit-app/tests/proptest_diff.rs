#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test: a failed setup or a bad slice is the assertion"
)]
//! Property tests for the two places that read free-form text from `git`: the
//! diff parser (`diff::parse_diff`) and the line-selection rewrite that stages
//! part of a hunk (`apply::transform_body`). Examples live in
//! `diff_parse.rs` and `apply_patch.rs`; these say what must hold for *any*
//! input, and shrink a failure to the smallest one.

use std::fmt::Write as _;
use std::ops::Range;

use ferrit_domain::apply::transform_body;
use ferrit_domain::diff::parse::{FileMeta, FileStatus};
use ferrit_domain::diff::parse_diff;
use proptest::prelude::*;

// ------------------------------------------------------------- the hunk body

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Context,
    Added,
    Deleted,
}

impl Kind {
    const fn prefix(self) -> char {
        match self {
            Self::Context => ' ',
            Self::Added => '+',
            Self::Deleted => '-',
        }
    }
}

fn kind() -> impl Strategy<Value = Kind> {
    prop_oneof![Just(Kind::Context), Just(Kind::Added), Just(Kind::Deleted)]
}

/// A hunk body as data: one `(kind, text)` per line.
fn lines() -> impl Strategy<Value = Vec<(Kind, String)>> {
    prop::collection::vec((kind(), "[a-z0-9 ]{0,12}"), 1..30)
}

fn render(lines: &[(Kind, String)], no_newline_at_end: bool) -> String {
    let mut out = String::new();
    for (kind, text) in lines {
        out.push(kind.prefix());
        out.push_str(text);
        out.push('\n');
    }
    if no_newline_at_end {
        out.push_str("\\ No newline at end of file\n");
    }
    out
}

/// A body and a selection mask of the same length.
fn body_and_selection() -> impl Strategy<Value = (Vec<(Kind, String)>, bool, Vec<bool>)> {
    lines().prop_flat_map(|lines| {
        let n = lines.len();
        (
            Just(lines),
            any::<bool>(),
            prop::collection::vec(any::<bool>(), n),
        )
    })
}

fn selected(mask: &[bool]) -> Vec<usize> {
    mask.iter()
        .enumerate()
        .filter_map(|(i, on)| on.then_some(i))
        .collect()
}

/// The text of the lines a patch reads from the old file: context and deleted.
fn old_side(patch: &str) -> Vec<&str> {
    patch
        .lines()
        .filter(|l| l.starts_with(' ') || l.starts_with('-'))
        .map(|l| &l[1..])
        .collect()
}

/// The text of the lines a patch leaves in the new file: context and added.
fn new_side(patch: &str) -> Vec<&str> {
    patch
        .lines()
        .filter(|l| l.starts_with(' ') || l.starts_with('+'))
        .map(|l| &l[1..])
        .collect()
}

proptest! {
    /// Selecting every line (changed or not) is a no-op: the patch is the hunk.
    #[test]
    fn selecting_everything_returns_the_body((lines, marker, _) in body_and_selection()) {
        let body = render(&lines, marker);
        let all: Vec<usize> = (0..lines.len()).collect();
        prop_assert_eq!(transform_body(&body, &all), body);
    }

    /// Whatever is selected, the rewritten patch still reads the same old file,
    /// so `git apply` can find its place: the old side never changes.
    #[test]
    fn the_old_side_never_changes((lines, marker, mask) in body_and_selection()) {
        let body = render(&lines, marker);
        let out = transform_body(&body, &selected(&mask));
        prop_assert_eq!(old_side(&out), old_side(&body));
    }

    /// The new side is exactly: every context line, every unselected deletion
    /// (kept as context), and the selected additions, in order.
    #[test]
    fn the_new_side_is_context_plus_the_chosen_changes(
        (lines, marker, mask) in body_and_selection()
    ) {
        let body = render(&lines, marker);
        let out = transform_body(&body, &selected(&mask));
        let expected: Vec<&str> = lines
            .iter()
            .zip(&mask)
            .filter(|((kind, _), on)| match kind {
                Kind::Context => true,
                Kind::Added => **on,
                Kind::Deleted => !**on,
            })
            .map(|((_, text), _)| text.as_str())
            .collect();
        prop_assert_eq!(new_side(&out), expected);
    }

    /// Selecting nothing leaves no change at all: no `+` line, no `-` line.
    #[test]
    fn selecting_nothing_leaves_no_change((lines, marker, _) in body_and_selection()) {
        let out = transform_body(&render(&lines, marker), &[]);
        prop_assert!(out.lines().all(|l| !l.starts_with('+') && !l.starts_with('-')));
    }

    /// An index past the end of the body is ignored, not a panic.
    #[test]
    fn out_of_range_selections_are_ignored((lines, marker, mask) in body_and_selection()) {
        let body = render(&lines, marker);
        let mut picks = selected(&mask);
        let plain = transform_body(&body, &picks);
        picks.push(lines.len() + 5);
        prop_assert_eq!(transform_body(&body, &picks), plain);
    }
}

// ------------------------------------------------------------ the diff parser

fn in_bounds(text: &str, range: &Range<usize>) -> bool {
    range.start <= range.end && text.get(range.clone()).is_some()
}

fn assert_ranges_are_valid(text: &str, files: &[FileMeta]) -> Result<(), TestCaseError> {
    let mut previous_end = 0;
    for file in files {
        prop_assert!(
            in_bounds(text, &file.header),
            "file header {:?}",
            file.header
        );
        prop_assert!(
            in_bounds(text, &file.old_path),
            "old path {:?}",
            file.old_path
        );
        prop_assert!(
            in_bounds(text, &file.new_path),
            "new path {:?}",
            file.new_path
        );
        prop_assert!(file.header.start >= previous_end, "files overlap");
        previous_end = file.header.end;
        let mut hunk_end = file.header.end;
        for hunk in &file.hunks {
            prop_assert!(
                in_bounds(text, &hunk.header),
                "hunk header {:?}",
                hunk.header
            );
            prop_assert!(in_bounds(text, &hunk.body), "hunk body {:?}", hunk.body);
            prop_assert!(
                hunk.header.start >= hunk_end,
                "hunks overlap or run backwards"
            );
            hunk_end = hunk.body.end.max(hunk.header.end);
        }
    }
    Ok(())
}

/// Lines that look like the pieces of a diff, mixed with noise.
fn diff_like_line() -> impl Strategy<Value = String> {
    prop_oneof![
        "diff --git a/[a-z/]{1,8} b/[a-z/]{1,8}",
        "index [0-9a-f]{7}\\.\\.[0-9a-f]{7} 100644",
        "--- (a/)?[a-z/]{1,8}",
        "\\+\\+\\+ (b/)?[a-z/]{1,8}",
        "@@ -[0-9]{1,4}(,[0-9]{1,3})? \\+[0-9]{1,4}(,[0-9]{1,3})? @@( [a-z ]{0,10})?",
        "[ +\\-][a-z0-9 ]{0,12}",
        Just("\\ No newline at end of file".to_owned()),
        Just("Binary files a/x and b/x differ".to_owned()),
        "(rename|copy) (from|to) [a-z/]{1,8}",
        "new file mode 100644",
        "deleted file mode 100644",
        ".{0,20}",
    ]
}

proptest! {
    /// Any text at all parses without panicking, and every range it reports
    /// slices the text on character boundaries, in order, without overlap.
    #[test]
    fn arbitrary_text_never_panics_and_ranges_stay_valid(text in ".{0,400}") {
        let diff = parse_diff(&text);
        assert_ranges_are_valid(&text, &diff.files)?;
    }

    #[test]
    fn diff_shaped_text_keeps_its_ranges_valid(
        parts in prop::collection::vec(diff_like_line(), 0..40),
        trailing_newline in any::<bool>(),
    ) {
        let mut text = parts.join("\n");
        if trailing_newline {
            text.push('\n');
        }
        let diff = parse_diff(&text);
        assert_ranges_are_valid(&text, &diff.files)?;
    }
}

#[derive(Debug, Clone)]
struct GenHunk {
    old_start: u32,
    new_start: u32,
    lines: Vec<(Kind, String)>,
}

impl GenHunk {
    fn old_count(&self) -> u32 {
        let count = self.lines.iter().filter(|(k, _)| *k != Kind::Added).count();
        u32::try_from(count).unwrap()
    }

    fn new_count(&self) -> u32 {
        let count = self
            .lines
            .iter()
            .filter(|(k, _)| *k != Kind::Deleted)
            .count();
        u32::try_from(count).unwrap()
    }

    /// The header as git prints it: a count of 1 is left out (`@@ -5 +5 @@`).
    fn header(&self) -> String {
        let range = |start: u32, count: u32| {
            if count == 1 {
                start.to_string()
            } else {
                format!("{start},{count}")
            }
        };
        format!(
            "@@ -{} +{} @@",
            range(self.old_start, self.old_count()),
            range(self.new_start, self.new_count())
        )
    }
}

fn hunk() -> impl Strategy<Value = GenHunk> {
    (1u32..2000, 1u32..2000, lines()).prop_map(|(old_start, new_start, lines)| GenHunk {
        old_start,
        new_start,
        lines,
    })
}

fn file() -> impl Strategy<Value = (String, Vec<GenHunk>)> {
    (
        "[a-z]{1,6}(/[a-z]{1,6}){0,2}\\.rs",
        prop::collection::vec(hunk(), 1..4),
    )
}

proptest! {
    /// A well-formed diff parses back to what generated it: the files, their
    /// paths, each hunk's header, counts and body.
    #[test]
    fn a_well_formed_diff_parses_back_to_what_made_it(
        files in prop::collection::vec(file(), 1..4)
    ) {
        let mut text = String::new();
        let mut bodies: Vec<Vec<String>> = Vec::new();
        for (name, hunks) in &files {
            writeln!(text, "diff --git a/{name} b/{name}").unwrap();
            text.push_str("index 1111111..2222222 100644\n");
            writeln!(text, "--- a/{name}\n+++ b/{name}").unwrap();
            let mut file_bodies = Vec::new();
            for hunk in hunks {
                text.push_str(&hunk.header());
                text.push('\n');
                let body = render(&hunk.lines, false);
                text.push_str(&body);
                file_bodies.push(body);
            }
            bodies.push(file_bodies);
        }

        let diff = parse_diff(&text);
        prop_assert_eq!(diff.files.len(), files.len());
        for (((name, hunks), parsed), file_bodies) in files.iter().zip(&diff.files).zip(&bodies) {
            prop_assert_eq!(&text[parsed.old_path.clone()], name.as_str());
            prop_assert_eq!(&text[parsed.new_path.clone()], name.as_str());
            prop_assert_eq!(parsed.status, FileStatus::Modified);
            prop_assert_eq!(parsed.hunks.len(), hunks.len());
            for ((generated, got), body) in hunks.iter().zip(&parsed.hunks).zip(file_bodies) {
                prop_assert_eq!(&text[got.header.clone()], generated.header());
                prop_assert_eq!(
                    (got.old_start, got.old_count, got.new_start, got.new_count),
                    (
                        generated.old_start,
                        generated.old_count(),
                        generated.new_start,
                        generated.new_count()
                    )
                );
                prop_assert_eq!(&text[got.body.clone()], body.as_str());
            }
        }
    }
}
