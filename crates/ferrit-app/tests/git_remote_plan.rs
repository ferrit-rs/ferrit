#![allow(
    clippy::panic,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! What `P` comes to, decided from the Status header (`git::remote::plan_push`).

use ferrit_domain::model::{RemoteEntry, StatusHeader};
use ferrit_domain::remote::{PushPlan, RemoteRequest, parse_upstream, plan_push};

fn header(upstream: Option<&str>, ahead: usize, behind: usize) -> StatusHeader {
    StatusHeader {
        branch: "feature".to_owned(),
        detached: false,
        upstream: upstream.map(str::to_owned),
        ahead,
        behind,
        conflicts: 0,
    }
}

fn remote(name: &str) -> RemoteEntry {
    RemoteEntry {
        name: name.to_owned(),
        fetch_url: String::new(),
        push_url: String::new(),
    }
}

#[test]
fn a_branch_that_is_not_behind_pushes_as_is() {
    assert_eq!(
        plan_push(&header(Some("origin/feature"), 2, 0), &[], false),
        PushPlan::Plain
    );
}

#[test]
fn a_branch_behind_its_upstream_asks_before_forcing() {
    let plan = plan_push(&header(Some("origin/feature"), 0, 3), &[], false);
    let PushPlan::ConfirmForce(message) = plan else {
        panic!("expected a question, got {plan:?}");
    };
    assert!(message.contains("behind upstream by 3"), "{message}");
}

#[test]
fn a_diverged_branch_says_so() {
    let plan = plan_push(&header(Some("origin/feature"), 1, 2), &[], false);
    let PushPlan::ConfirmForce(message) = plan else {
        panic!("expected a question, got {plan:?}");
    };
    assert!(message.contains("ahead 1, behind 2"), "{message}");
}

#[test]
fn push_default_current_sets_the_upstream_without_asking() {
    assert_eq!(
        plan_push(&header(None, 1, 0), &[remote("origin")], true),
        PushPlan::SetCurrent
    );
}

#[test]
fn without_an_upstream_the_prompt_starts_from_origin_then_the_first_remote() {
    assert_eq!(
        plan_push(
            &header(None, 1, 0),
            &[remote("fork"), remote("origin")],
            false
        ),
        PushPlan::AskUpstream("origin feature".to_owned())
    );
    assert_eq!(
        plan_push(&header(None, 1, 0), &[remote("fork")], false),
        PushPlan::AskUpstream("fork feature".to_owned())
    );
    assert_eq!(
        plan_push(&header(None, 1, 0), &[], false),
        PushPlan::AskUpstream("origin feature".to_owned())
    );
}

#[test]
fn the_upstream_prompt_wants_exactly_two_words() {
    assert_eq!(parse_upstream("origin main"), Some(("origin", "main")));
    assert_eq!(parse_upstream("  origin   main "), Some(("origin", "main")));
    assert_eq!(parse_upstream("origin"), None);
    assert_eq!(parse_upstream("origin main extra"), None);
    assert_eq!(parse_upstream(""), None);
}

#[test]
fn a_request_for_a_push_to_a_remote_sets_both_names() {
    let request = RemoteRequest::push_to("origin".to_owned(), "main".to_owned());
    assert_eq!(request.push_upstream.as_deref(), Some("origin"));
    assert_eq!(request.upstream_branch.as_deref(), Some("main"));
    assert!(!request.force_with_lease);
}
