#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! `git::config` parsing and `ConfigView` lookups on captured
//! `git config --list --show-origin --show-scope -z` output.
//! See `docs/PLAN_14_GIT_CONFIG.md` milestone G0.

use ferrit::domain::git::config::{Origin, Scope, parse};

const SAMPLE: &str = "system\0file:/etc/gitconfig\0core.autocrlf\ninput\0\
global\0file:/home/u/.gitconfig\0pull.rebase\ntrue\0\
global\0file:/home/u/.gitconfig\0core.editor\nnvim\0\
local\0file:.git/config\0pull.rebase\nmerges\0\
local\0file:.git/config\0a.b\nx\ny\0\
local\0file:.git/config\0a.noeq\0\
local\0file:.git/config\0a.empty\n\0\
local\0file:.git/inc\0inc.k\nv\0\
command\0command line:\0user.name\ncli\0";

#[test]
fn parses_every_scope_origin_and_value_shape() {
    let view = parse(SAMPLE);
    assert_eq!(view.entries.len(), 9);
    let first = &view.entries[0];
    assert_eq!(first.scope, Scope::System);
    assert_eq!(first.origin, Origin::File("/etc/gitconfig".into()));
    let multiline = view.effective("a.b").unwrap();
    assert_eq!(multiline.value, "x\ny");
    assert_eq!(view.effective("a.noeq").unwrap().value, "");
    assert_eq!(view.effective("a.empty").unwrap().value, "");
    assert_eq!(
        view.effective("inc.k").unwrap().origin,
        Origin::File(".git/inc".into())
    );
    let cli = view.effective("user.name").unwrap();
    assert_eq!(
        (cli.scope, &cli.origin),
        (Scope::Command, &Origin::CommandLine)
    );
}

#[test]
fn the_last_value_wins_and_the_earlier_ones_are_shadowed() {
    let view = parse(SAMPLE);
    let winner = view.effective("pull.rebase").unwrap();
    assert_eq!(
        (winner.scope, winner.value.as_str()),
        (Scope::Local, "merges")
    );
    let hidden: Vec<_> = view.shadowed("pull.rebase").collect();
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].scope, Scope::Global);
    assert_eq!(view.shadowed("core.editor").count(), 0);
    assert!(view.effective("nope.nope").is_none());
}

#[test]
fn empty_and_truncated_input_do_not_panic() {
    assert!(parse("").entries.is_empty());
    assert!(parse("local\0file:x\0").entries.is_empty());
}

#[test]
fn repo_config_reads_the_local_scope_of_a_real_repository() {
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("ferrit-gitconfig-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for args in [
        &["init", "-q", "."][..],
        &["config", "--local", "pull.rebase", "merges"],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }
    // Read-only: whatever the user's own global file says, the local value is the last word.
    let view = ferrit::domain::git::Repo::open(&dir)
        .unwrap()
        .config()
        .unwrap();
    let winner = view.effective("pull.rebase").unwrap();
    assert_eq!(
        (winner.scope, winner.value.as_str()),
        (Scope::Local, "merges")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
