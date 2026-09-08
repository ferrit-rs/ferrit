#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test: a failed setup or a bad slice is the assertion"
)]
//! Parser coverage for `ferrit::git::parse_diff`: feed it canned plain-text
//! `git diff` output and check the byte ranges slice back to the right spans,
//! rename detection fires, and the hunk-header numbers come out.

use ferrit::git::{FileStatus, parse_diff};

/// Two files in one diff: a modified source file with two hunks, and a rename
/// with a small edit. Trailing `\ No newline at end of file` on the first.
const SAMPLE: &str = "\
diff --git a/src/main.rs b/src/main.rs
index 1111111..2222222 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"old\");
+    println!(\"new\");
+    do_more();
 }
@@ -20,2 +21,2 @@ fn tail() {
-    left();
+    right();
\\ No newline at end of file
diff --git a/old/name.txt b/new/name.txt
similarity index 96%
rename from old/name.txt
rename to new/name.txt
index 3333333..4444444 100644
--- a/old/name.txt
+++ b/new/name.txt
@@ -1 +1 @@
-hello
+hallo
";

#[test]
fn ranges_slice_back_to_their_text() {
    let diff = parse_diff(SAMPLE);
    assert_eq!(diff.files.len(), 2, "two file sections");

    let f0 = &diff.files[0];
    assert_eq!(&SAMPLE[f0.old_path.clone()], "src/main.rs");
    assert_eq!(&SAMPLE[f0.new_path.clone()], "src/main.rs");
    assert_eq!(f0.status, FileStatus::Modified);
    assert!(!f0.binary);
    assert_eq!(f0.hunks.len(), 2, "main.rs has two hunks");

    let h0 = &f0.hunks[0];
    assert_eq!(&SAMPLE[h0.header.clone()], "@@ -1,3 +1,4 @@");
    assert_eq!(
        (h0.old_start, h0.old_count, h0.new_start, h0.new_count),
        (1, 3, 1, 4)
    );
    assert!(
        SAMPLE[h0.body.clone()].contains("do_more();"),
        "hunk body carries the added line"
    );
}

#[test]
fn missing_hunk_counts_default_to_one() {
    let diff = parse_diff(SAMPLE);
    let h = &diff.files[1].hunks[0];
    assert_eq!(&SAMPLE[h.header.clone()], "@@ -1 +1 @@");
    assert_eq!(
        (h.old_start, h.old_count, h.new_start, h.new_count),
        (1, 1, 1, 1),
        "`@@ -1 +1 @@` means one line each side"
    );
}

#[test]
fn rename_is_detected_with_both_paths() {
    let diff = parse_diff(SAMPLE);
    let f = &diff.files[1];
    assert_eq!(f.status, FileStatus::Renamed);
    assert_eq!(&SAMPLE[f.old_path.clone()], "old/name.txt");
    assert_eq!(&SAMPLE[f.new_path.clone()], "new/name.txt");
}

#[test]
fn no_newline_marker_stays_in_the_hunk_body() {
    let diff = parse_diff(SAMPLE);
    let last = diff.files[0].hunks.last().unwrap();
    assert!(SAMPLE[last.body.clone()].contains("\\ No newline at end of file"));
}

#[test]
fn hunk_and_file_line_indices_are_in_order() {
    let diff = parse_diff(SAMPLE);
    let files = diff.file_lines();
    let hunks = diff.hunk_lines();
    assert_eq!(files.len(), 2);
    assert_eq!(hunks.len(), 3);
    assert!(files.windows(2).all(|w| w[0] < w[1]), "file headers ascend");
    assert!(hunks.windows(2).all(|w| w[0] < w[1]), "hunk headers ascend");
    assert!(
        hunks[0] > files[0] && hunks[2] > files[1],
        "each hunk sits below its file header"
    );
}

#[test]
fn empty_text_yields_no_files() {
    assert!(parse_diff("").files.is_empty());
    assert!(parse_diff("not a diff at all\n").files.is_empty());
}
