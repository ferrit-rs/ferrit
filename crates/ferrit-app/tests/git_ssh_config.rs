#![allow(
    clippy::unwrap_used,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! `git::ssh_config`: the GitHub host aliases of an ssh config
//! (`docs/PLAN_15_CREATE_REMOTE.md`).

use ferrit_config::ssh_config::{github_aliases, read_github_aliases};

#[test]
fn the_users_own_config_shape_gives_its_alias() {
    let config = "# personal account\nHost github.com-personal\n  HostName github.com\n  User git   \n  IdentityFile ~/.ssh/github-personal\n";
    assert_eq!(github_aliases(config), ["github.com-personal"]);
}

#[test]
fn several_accounts_give_every_alias_in_order() {
    let config = "\
Host github-work
    HostName github.com
    IdentityFile ~/.ssh/work

Host gl
    HostName gitlab.com

Host github-me
    HostName github.com
";
    assert_eq!(github_aliases(config), ["github-work", "github-me"]);
}

#[test]
fn keywords_are_case_insensitive_and_may_use_an_equals_sign() {
    let config = "host Alias-One\n  hostname = GitHub.com\nHOST=alias-two\nHostName=github.com\n";
    assert_eq!(github_aliases(config), ["Alias-One", "alias-two"]);
}

#[test]
fn several_patterns_on_one_line_each_count_and_wildcards_do_not() {
    let config = "Host a b *.example !c d?\n  HostName github.com\n";
    assert_eq!(github_aliases(config), ["a", "b"]);
}

#[test]
fn github_com_itself_and_other_hosts_are_not_aliases() {
    let config = "Host github.com\n  HostName github.com\n  IdentityFile ~/.ssh/x\nHost *\n  HostName github.com\nHost other\n  HostName example.com\n";
    assert!(github_aliases(config).is_empty());
}

#[test]
fn a_match_block_ends_the_host_block_before_it() {
    let config = "Host mine\nMatch host github.com\n  HostName github.com\n";
    assert!(
        github_aliases(config).is_empty(),
        "the HostName belongs to the Match"
    );
}

#[test]
fn a_duplicate_is_listed_once_and_comments_and_blanks_are_skipped() {
    let config = "\n# a comment\nHost dup\n  HostName github.com\n\nHost dup\n  # HostName elsewhere\n  HostName github.com\n";
    assert_eq!(github_aliases(config), ["dup"]);
}

#[test]
fn a_missing_file_has_no_aliases_and_a_real_one_is_read() {
    let dir = std::env::temp_dir().join(format!("ferrit-sshcfg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(read_github_aliases(&dir.join("absent")).is_empty());
    let file = dir.join("config");
    std::fs::write(&file, "Host me\n  HostName github.com\n").unwrap();
    assert_eq!(read_github_aliases(&file), ["me"]);
    let _ = std::fs::remove_dir_all(&dir);
}
