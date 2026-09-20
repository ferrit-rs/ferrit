//! Unit coverage for `git::apply::transform_body`, the line-selection patch
//! transform, in isolation: a known hunk body plus a line-index set in, a
//! byte-for-byte patch body out. No repo, no `git` subprocess. See
//! `docs/PLAN_6_STAGING.md` milestone S2.

use ferrit::git::apply::transform_body;

#[test]
fn selected_addition_is_kept_the_rest_of_the_hunk_is_untouched() {
    let body = " ctx\n-old\n+new1\n+new2\n ctx2\n";
    // Select only the first `+` (index 2): the unselected `-` demotes to
    // context, the unselected `+` is dropped entirely.
    let out = transform_body(body, &[2]);
    assert_eq!(out, " ctx\n old\n+new1\n ctx2\n");
}

#[test]
fn selecting_the_removal_keeps_it_and_drops_every_addition() {
    let body = "-old\n+new1\n+new2\n";
    let out = transform_body(body, &[0]);
    assert_eq!(out, "-old\n");
}

#[test]
fn selecting_every_plus_minus_line_reproduces_the_hunk_verbatim() {
    let body = " ctx\n-old\n+new\n";
    let out = transform_body(body, &[1, 2]);
    assert_eq!(out, body);
}

#[test]
fn no_newline_marker_follows_the_line_above() {
    let body = "-old\n+kept\n+dropped\n\\ No newline at end of file\n";
    // The trailing marker belongs to "+dropped": drop it too. "-old" is not
    // selected either, so it demotes to context.
    let out = transform_body(body, &[1]);
    assert_eq!(out, " old\n+kept\n");

    // Selecting the last `+` instead keeps the marker with it.
    let out = transform_body(body, &[2]);
    assert_eq!(out, " old\n+dropped\n\\ No newline at end of file\n");
}

#[test]
fn empty_selection_demotes_every_removal_and_drops_every_addition() {
    let body = "-a\n-b\n+c\n";
    let out = transform_body(body, &[]);
    assert_eq!(out, " a\n b\n");
}
