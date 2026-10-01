#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! `git::host`: what is typed is checked before anything runs, and `gh`'s
//! arguments come from the checked fields only (`docs/PLAN_15_CREATE_REMOTE.md`,
//! R0). No `gh` runs here.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use ferrit::domain::git::command_log::{CommandKind, recent};
use ferrit::domain::git::host::{
    CreateRequest, GhProgram, GhStatus, HostError, Visibility, build_create_args, gh_status,
    parse_target, sanitize_name, validate_description,
};

fn request(
    owner: Option<&str>,
    name: &str,
    visibility: Visibility,
    description: &str,
) -> CreateRequest {
    CreateRequest {
        owner: owner.map(str::to_owned),
        name: name.to_owned(),
        visibility,
        description: description.to_owned(),
    }
}

fn args(req: &CreateRequest) -> Vec<String> {
    build_create_args(req, Path::new("/work/ferrit"))
        .unwrap()
        .into_iter()
        .map(|a: OsString| a.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn a_private_repository_for_the_signed_in_account() {
    assert_eq!(
        args(&request(None, "ferrit", Visibility::Private, "")),
        [
            "repo",
            "create",
            "ferrit",
            "--private",
            "--source",
            "/work/ferrit",
            "--remote",
            "origin"
        ]
    );
}

#[test]
fn public_organisation_and_description_are_each_in_the_argument_list() {
    assert_eq!(
        args(&request(
            Some("acme"),
            "tool",
            Visibility::Public,
            "A small tool"
        )),
        [
            "repo",
            "create",
            "acme/tool",
            "--public",
            "--source",
            "/work/ferrit",
            "--remote",
            "origin",
            "--description",
            "A small tool"
        ]
    );
}

#[test]
fn the_arguments_never_push_initialise_or_license() {
    for visibility in [Visibility::Private, Visibility::Public] {
        let all = args(&request(Some("acme"), "tool", visibility, "d"));
        for forbidden in [
            "--push",
            "--add-readme",
            "--gitignore",
            "--license",
            "--template",
            "--clone",
        ] {
            assert!(
                !all.iter().any(|a| a == forbidden),
                "{forbidden} in {all:?}"
            );
        }
    }
}

#[test]
fn a_description_with_dashes_stays_one_argument() {
    let all = args(&request(
        None,
        "x",
        Visibility::Private,
        "--push everything",
    ));
    assert_eq!(all.last().unwrap(), "--push everything");
    assert_eq!(all[all.len() - 2], "--description");
}

#[test]
fn names_follow_githubs_rule() {
    for ok in ["ferrit", "a", "my-repo_2.0", ".github", "A.B-c_d"] {
        assert_eq!(parse_target(ok).unwrap(), (None, ok.to_owned()), "{ok}");
    }
    assert_eq!(parse_target("").unwrap_err(), HostError::EmptyName);
    assert_eq!(parse_target("   ").unwrap_err(), HostError::EmptyName);
    assert_eq!(
        parse_target("my repo").unwrap_err(),
        HostError::NameChar(' ')
    );
    assert_eq!(parse_target("café").unwrap_err(), HostError::NameChar('é'));
    assert_eq!(parse_target("a?b").unwrap_err(), HostError::NameChar('?'));
    assert_eq!(
        parse_target(".").unwrap_err(),
        HostError::NameReserved(".".into())
    );
    assert_eq!(
        parse_target("..").unwrap_err(),
        HostError::NameReserved("..".into())
    );
    assert!(parse_target(&"x".repeat(100)).is_ok());
    assert_eq!(
        parse_target(&"x".repeat(101)).unwrap_err(),
        HostError::NameTooLong
    );
}

#[test]
fn an_owner_is_the_part_before_the_slash() {
    assert_eq!(
        parse_target("  acme/tool ").unwrap(),
        (Some("acme".into()), "tool".into())
    );
    assert_eq!(parse_target("/tool").unwrap_err(), HostError::EmptyOwner);
    assert_eq!(parse_target("acme/").unwrap_err(), HostError::EmptyName);
    assert_eq!(
        parse_target("a/b/c").unwrap_err(),
        HostError::TooManySlashes
    );
    assert_eq!(
        parse_target("ac_me/tool").unwrap_err(),
        HostError::OwnerChar('_')
    );
    assert_eq!(
        parse_target(&format!("{}/x", "o".repeat(40))).unwrap_err(),
        HostError::OwnerTooLong
    );
}

#[test]
fn the_description_is_one_line_of_at_most_350_characters() {
    assert!(validate_description("").is_ok());
    assert!(validate_description(&"é".repeat(350)).is_ok());
    assert_eq!(
        validate_description(&"x".repeat(351)).unwrap_err(),
        HostError::DescriptionTooLong
    );
    assert_eq!(
        validate_description("a\nb").unwrap_err(),
        HostError::DescriptionLines
    );
    assert_eq!(
        validate_description("a\rb").unwrap_err(),
        HostError::DescriptionLines
    );
}

#[test]
fn bad_fields_never_reach_the_argument_list() {
    let workdir = Path::new("/w");
    let bad = [
        request(None, "my repo", Visibility::Private, ""),
        request(None, "", Visibility::Private, ""),
        request(Some("a_b"), "ok", Visibility::Private, ""),
        request(Some(""), "ok", Visibility::Private, ""),
        request(None, "ok", Visibility::Private, "two\nlines"),
    ];
    for req in &bad {
        assert!(build_create_args(req, workdir).is_err(), "{req:?}");
    }
}

#[test]
fn a_folder_name_becomes_a_usable_default() {
    assert_eq!(sanitize_name("ferrit"), "ferrit");
    assert_eq!(sanitize_name("my project"), "my-project");
    assert_eq!(sanitize_name("  Café   Ünï  "), "Caf-n");
    assert_eq!(sanitize_name("a  b"), "a-b");
    assert_eq!(sanitize_name("---x---"), "x");
    assert_eq!(sanitize_name("日本語"), "repo");
    assert_eq!(sanitize_name(""), "repo");
    assert_eq!(sanitize_name(".."), "repo");
    assert_eq!(sanitize_name(&"y".repeat(150)).len(), 100);
    for folder in ["my project", "Café", "a/b", "x y z", "über-repo"] {
        assert!(parse_target(&sanitize_name(folder)).is_ok(), "{folder}");
    }
}

/// A fake `gh` in a temp directory: it appends its arguments to `calls.log`,
/// says the version, and fails `auth status` while `signed-out` exists.
struct FakeGh {
    dir: PathBuf,
}

impl FakeGh {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("gh");
        fs::write(
            &script,
            "#!/bin/sh\n\
             here=$(dirname \"$0\")\n\
             echo \"$@\" >> \"$here/calls.log\"\n\
             case \"$1\" in\n\
               --version) echo 'gh version 2.50.0'; exit 0 ;;\n\
               auth) if [ -f \"$here/signed-out\" ]; then echo 'You are not logged in' >&2; exit 1; fi; exit 0 ;;\n\
             esac\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn program(&self) -> GhProgram {
        GhProgram::new(self.dir.join("gh"))
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for FakeGh {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_signed_in_gh_is_ready_after_exactly_two_calls() {
    let fake = FakeGh::new("host-ready");
    assert_eq!(gh_status(&fake.program()), GhStatus::Ready);
    assert_eq!(fake.calls(), ["--version", "auth status"]);
}

#[test]
fn a_signed_out_gh_says_so() {
    let fake = FakeGh::new("host-out");
    fs::write(fake.dir.join("signed-out"), "").unwrap();
    assert_eq!(gh_status(&fake.program()), GhStatus::SignedOut);
}

#[test]
fn a_missing_gh_stops_at_the_first_call() {
    let nowhere = GhProgram::new("/nonexistent/ferrit-test/gh");
    assert_eq!(gh_status(&nowhere), GhStatus::Missing);

    let fake = FakeGh::new("host-broken");
    fs::write(fake.dir.join("gh"), "#!/bin/sh\nexit 3\n").unwrap();
    assert_eq!(
        gh_status(&fake.program()),
        GhStatus::Missing,
        "it runs but fails"
    );
}

#[test]
fn the_status_checks_are_logged_as_reads_under_gh_s_name() {
    let fake = FakeGh::new("host-log");
    assert_eq!(gh_status(&fake.program()), GhStatus::Ready);
    let records = recent(usize::MAX, true);
    for argv in ["gh --version", "gh auth status"] {
        let found: Vec<_> = records.iter().filter(|r| r.argv == argv).collect();
        assert!(!found.is_empty(), "{argv} not logged");
        // The log is process-wide and the other tests here make `gh` fail on
        // purpose, so every record reads and at least this run's succeeded.
        assert!(found.iter().all(|r| r.kind == CommandKind::Read));
        assert!(found.iter().any(|r| r.exit == Some(0)));
    }
}
