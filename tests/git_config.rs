#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! `git::config` parsing and `ConfigView` lookups on captured
//! `git config --list --show-origin --show-scope -z` output.
//! See `docs/PLAN_14_GIT_CONFIG.md` milestone G0.

use ferrit::domain::git::Repo;
use ferrit::domain::git::config::{
    Origin, Scope, ValueKind, WriteScope, display_value, is_secret_key, parse,
};

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
    let view = Repo::open(&dir).unwrap().config().unwrap();
    let winner = view.effective("pull.rebase").unwrap();
    assert_eq!(
        (winner.scope, winner.value.as_str()),
        (Scope::Local, "merges")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repo_writes_local_config_through_git_and_reads_it_back() {
    let dir = std::env::temp_dir().join(format!("ferrit-gitconfig-w-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["init", "-q", "."])
        .status()
        .unwrap();
    assert!(status.success());
    let repo = Repo::open(&dir).unwrap();
    let local = |key: &str| {
        let view = repo.config().unwrap();
        view.entries
            .iter()
            .filter(|e| e.scope == Scope::Local && e.key == key)
            .map(|e| e.value.clone())
            .collect::<Vec<_>>()
    };

    repo.config_set(WriteScope::Local, "commit.gpgsign", "on", ValueKind::Bool)
        .unwrap();
    assert_eq!(local("commit.gpgsign"), ["true"]);
    repo.config_add(
        WriteScope::Local,
        "remote.origin.fetch",
        "a",
        ValueKind::Text,
    )
    .unwrap();
    repo.config_add(
        WriteScope::Local,
        "remote.origin.fetch",
        "b",
        ValueKind::Text,
    )
    .unwrap();
    assert_eq!(local("remote.origin.fetch"), ["a", "b"]);
    repo.config_replace_all(
        WriteScope::Local,
        "remote.origin.fetch",
        "c",
        ValueKind::Text,
    )
    .unwrap();
    assert_eq!(local("remote.origin.fetch"), ["c"]);
    repo.config_unset(WriteScope::Local, "commit.gpgsign")
        .unwrap();
    assert!(local("commit.gpgsign").is_empty());
    assert!(
        repo.config_unset(WriteScope::Local, "commit.gpgsign")
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn known_keys_are_real_git_keys_and_their_values_are_accepted_by_git() {
    use ferrit::domain::git::config_keys::{KNOWN_KEYS, KeyType, lookup};

    let out = std::process::Command::new("git")
        .args(["help", "-c"])
        .output()
        .unwrap();
    let real: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_lowercase)
        .collect();
    assert!(real.len() > 100, "`git help -c` listed {} keys", real.len());

    let dir = std::env::temp_dir().join(format!("ferrit-gitconfig-k-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["init", "-q", "."])
        .status()
        .unwrap();
    assert!(status.success());
    let repo = Repo::open(&dir).unwrap();

    for known in KNOWN_KEYS {
        assert!(
            real.iter().any(|k| k == known.key),
            "{} is not a git key",
            known.key
        );
        assert_eq!(lookup(&known.key.to_uppercase()), Some(known));
        let samples: Vec<&str> = match known.kind {
            KeyType::Bool => vec!["true", "false"],
            KeyType::Enum(values) => values.to_vec(),
            KeyType::Text => vec!["sample"],
        };
        for value in samples {
            let written =
                repo.config_set(WriteScope::Local, known.key, value, known.kind.value_kind());
            assert!(written.is_ok(), "{} = {value}: {written:?}", known.key);
        }
    }
    assert!(lookup("url.x.insteadof").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn one_value_of_a_multi_valued_key_changes_or_goes_alone() {
    let dir = std::env::temp_dir().join(format!("ferrit-gitconfig-v-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["init", "-q", "."])
        .status()
        .unwrap();
    assert!(status.success());
    let repo = Repo::open(&dir).unwrap();
    let local = || {
        repo.config()
            .unwrap()
            .entries
            .into_iter()
            .filter(|e| e.scope == Scope::Local && e.key == "credential.helper")
            .map(|e| e.value)
            .collect::<Vec<_>>()
    };
    for value in ["a.*", "b"] {
        repo.config_add(
            WriteScope::Local,
            "credential.helper",
            value,
            ValueKind::Text,
        )
        .unwrap();
    }
    // `a.*` would match `b` as a pattern: `--fixed-value` makes it literal.
    repo.config_replace_value(
        WriteScope::Local,
        "credential.helper",
        "c",
        "a.*",
        ValueKind::Text,
    )
    .unwrap();
    assert_eq!(local(), ["c", "b"]);
    repo.config_unset_value(WriteScope::Local, "credential.helper", "b")
        .unwrap();
    assert_eq!(local(), ["c"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_value_from_an_included_file_is_told_apart_from_the_main_file() {
    let view = parse(
        "local\0file:.git/config\0core.bare\nfalse\0\
local\0file:.git/config\0include.path\ninc\0\
local\0file:.git/inc\0inc.k\nv\0\
global\0file:/home/u/.gitconfig\0pull.rebase\ntrue\0",
    );
    let flags: Vec<bool> = view.entries.iter().map(|e| view.is_included(e)).collect();
    assert_eq!(flags, [false, false, true, false]);
}

#[test]
fn secrets_are_hidden_on_screen_and_in_the_command_log() {
    use ferrit::domain::git::command_log::recent;

    assert!(is_secret_key("http.proxyPassword"));
    assert!(is_secret_key("credential.https://x.example.token"));
    assert!(!is_secret_key("credential.helper"));
    assert!(!is_secret_key("pull.rebase"));
    assert_eq!(display_value("github.token", "ghp_abc"), "***");
    assert_eq!(display_value("github.token", ""), "");
    assert_eq!(
        display_value("remote.origin.url", "https://me:pw@host/r.git"),
        "https://me:***@host/r.git"
    );
    assert_eq!(display_value("core.editor", "nvim"), "nvim");

    let dir = std::env::temp_dir().join(format!("ferrit-gitconfig-s-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["init", "-q", "."])
        .status()
        .unwrap();
    assert!(status.success());
    let repo = Repo::open(&dir).unwrap();
    repo.config_set(
        WriteScope::Local,
        "zz.apitoken",
        "hunter2-first",
        ValueKind::Text,
    )
    .unwrap();
    repo.config_replace_value(
        WriteScope::Local,
        "zz.apitoken",
        "hunter2-second",
        "hunter2-first",
        ValueKind::Text,
    )
    .unwrap();
    let _ = repo.config().unwrap();

    let logged: Vec<_> = recent(usize::MAX, true)
        .into_iter()
        .filter(|r| r.argv.contains("zz.apitoken"))
        .collect();
    assert_eq!(logged.len(), 2, "{logged:?}");
    for record in &logged {
        assert!(!record.argv.contains("hunter2"), "{}", record.argv);
        assert!(record.argv.ends_with("***"), "{}", record.argv);
    }
    // The listing carries every secret on stdout: it is a read, so the log keeps none of it.
    let listing = recent(usize::MAX, true)
        .into_iter()
        .rev()
        .find(|r| r.argv.contains("--show-origin"))
        .unwrap();
    assert!(listing.output.is_empty());
    assert_eq!(
        listing.kind,
        ferrit::domain::git::command_log::CommandKind::Read
    );
    let _ = std::fs::remove_dir_all(&dir);
}
