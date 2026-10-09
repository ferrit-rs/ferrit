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

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ferrit::domain::git::command_log::{CommandKind, recent};
use ferrit::domain::git::error::GitError;
use ferrit::domain::git::host::{
    CreateRequest, GhProgram, GhStatus, HostError, Visibility, build_create_args, gh_status,
    parse_target, sanitize_name, ssh_remote_url, validate_description, validate_ssh_host,
};
use ferrit::infra::git::Repo;

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
               --version) [ -f \"$here/slow-version\" ] && sleep 30; echo 'gh version 2.50.0'; exit 0 ;;\n\
               auth) if [ -f \"$here/signed-out\" ]; then echo 'You are not logged in' >&2; exit 1; fi; exit 0 ;;\n\
               repo)\n\
                 [ -f \"$here/sleep\" ] && sleep 30\n\
                 if [ -f \"$here/name-taken\" ]; then echo 'GraphQL: Name already exists on this account (createRepository)' >&2; exit 1; fi\n\
                 target=\"$3\"\n\
                 while [ $# -gt 0 ]; do [ \"$1\" = --source ] && src=\"$2\"; shift; done\n\
                 git -C \"$src\" remote add origin \"https://github.com/$target.git\"\n\
                 echo \"https://github.com/$target\"; exit 0 ;;\n\
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

/// A repository to create from, next to its fake `gh`.
struct Project {
    fake: FakeGh,
    work: PathBuf,
}

impl Project {
    fn new(tag: &str) -> Self {
        let fake = FakeGh::new(tag);
        let work = fake.dir.join("work");
        fs::create_dir_all(&work).unwrap();
        // What git reports for the working tree: the real path, not a symlink.
        let work = fs::canonicalize(work).unwrap();
        let status = Command::new("git")
            .arg("-C")
            .arg(&work)
            .args(["init", "-q", "."])
            .status()
            .unwrap();
        assert!(status.success());
        Self { fake, work }
    }

    fn repo(&self) -> Repo {
        Repo::open(&self.work).unwrap()
    }

    fn remote_url(&self) -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.work)
            .args(["remote", "get-url", "origin"])
            .output()
            .unwrap();
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    /// The `gh repo create` calls the fake saw.
    fn creates(&self) -> Vec<String> {
        self.fake
            .calls()
            .into_iter()
            .filter(|c| c.starts_with("repo "))
            .collect()
    }
}

fn cancel_flag() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn creating_runs_gh_with_the_checked_arguments_and_returns_the_web_url() {
    let project = Project::new("host-create");
    let req = request(Some("acme"), "tool", Visibility::Private, "A small tool");
    let created = project
        .repo()
        .create_repo(&project.fake.program(), &req, &cancel_flag())
        .unwrap();

    assert_eq!(created.web_url, "https://github.com/acme/tool");
    assert_eq!(
        project.creates(),
        [format!(
            "repo create acme/tool --private --source {}/ --remote origin --description A small tool",
            project.work.display()
        )]
    );
    // `gh` adds the remote itself, and nothing was pushed or initialised.
    assert_eq!(
        project.remote_url().as_deref(),
        Some("https://github.com/acme/tool.git")
    );
    assert!(!project.creates()[0].contains("--push"));
}

#[test]
fn a_public_request_passes_public() {
    let project = Project::new("host-public");
    let req = request(None, "tool", Visibility::Public, "");
    project
        .repo()
        .create_repo(&project.fake.program(), &req, &cancel_flag())
        .unwrap();
    assert!(project.creates()[0].starts_with("repo create tool --public "));
}

#[test]
fn a_name_gh_refuses_comes_back_as_its_message_and_nothing_is_configured() {
    let project = Project::new("host-taken");
    fs::write(project.fake.dir.join("name-taken"), "").unwrap();
    let req = request(None, "tool", Visibility::Private, "");
    let err = project
        .repo()
        .create_repo(&project.fake.program(), &req, &cancel_flag())
        .unwrap_err();
    assert!(
        matches!(&err, GitError::HostFailed(m) if m.contains("Name already exists on this account")),
        "{err:?}"
    );
    assert_eq!(project.remote_url(), None);
}

#[test]
fn a_bad_target_or_an_existing_origin_never_reaches_gh() {
    let project = Project::new("host-guard");
    let repo = project.repo();
    let bad = request(None, "my repo", Visibility::Private, "");
    let err = repo
        .create_repo(&project.fake.program(), &bad, &cancel_flag())
        .unwrap_err();
    assert!(
        matches!(&err, GitError::HostFailed(m) if m.contains("not allowed in a name")),
        "{err:?}"
    );

    let status = Command::new("git")
        .arg("-C")
        .arg(&project.work)
        .args(["remote", "add", "origin", "https://example.com/x.git"])
        .status()
        .unwrap();
    assert!(status.success());
    let ok = request(None, "tool", Visibility::Private, "");
    let err = project
        .repo()
        .create_repo(&project.fake.program(), &ok, &cancel_flag())
        .unwrap_err();
    assert!(
        matches!(&err, GitError::HostFailed(m) if m.contains("origin already exists")),
        "{err:?}"
    );
    assert!(
        project.fake.calls().is_empty(),
        "gh was never called: {:?}",
        project.fake.calls()
    );
}

#[test]
fn a_cancelled_creation_stops_gh_and_configures_nothing() {
    let project = Project::new("host-cancel");
    fs::write(project.fake.dir.join("sleep"), "").unwrap();
    let cancel = std::sync::Arc::new(cancel_flag());
    let flag = std::sync::Arc::clone(&cancel);
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let req = request(None, "tool", Visibility::Private, "");
    let err = project
        .repo()
        .create_repo(&project.fake.program(), &req, &cancel)
        .unwrap_err();
    stopper.join().unwrap();
    assert!(
        matches!(&err, GitError::HostFailed(m) if m.contains("cancelled")),
        "{err:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "gh was left running"
    );
    assert_eq!(project.remote_url(), None);
}

#[test]
fn a_creation_that_outlasts_the_timeout_fails_and_configures_nothing() {
    let project = Project::new("host-timeout");
    fs::write(project.fake.dir.join("sleep"), "").unwrap();
    let gh = project.fake.program().with_timeout(Duration::from_secs(1));
    let started = Instant::now();
    let req = request(None, "tool", Visibility::Private, "");
    let err = project
        .repo()
        .create_repo(&gh, &req, &cancel_flag())
        .unwrap_err();
    assert!(
        matches!(&err, GitError::HostFailed(m) if m.contains("timed out after 1 seconds")),
        "{err:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(project.remote_url(), None);
}

#[test]
fn a_gh_that_hangs_on_its_status_check_reads_as_missing() {
    let fake = FakeGh::new("host-slow");
    fs::write(fake.dir.join("slow-version"), "").unwrap();
    let started = Instant::now();
    let gh = fake.program().with_timeout(Duration::from_secs(1));
    assert_eq!(gh_status(&gh), GhStatus::Missing);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn the_create_call_is_logged_as_a_write_under_gh_s_name() {
    let project = Project::new("host-logged");
    let req = request(None, "logged-tool", Visibility::Private, "");
    project
        .repo()
        .create_repo(&project.fake.program(), &req, &cancel_flag())
        .unwrap();
    let mark = format!("--source {}/", project.work.display());
    let found: Vec<_> = recent(usize::MAX, true)
        .into_iter()
        .filter(|r| r.argv.starts_with("gh repo create logged-tool") && r.argv.contains(&mark))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found.first().map(|r| r.kind), Some(CommandKind::Write));
    assert_eq!(found.first().and_then(|r| r.exit), Some(0));
}

#[test]
fn an_ssh_host_is_a_name_ssh_config_could_hold() {
    for ok in ["github.com-personal", "gh_work", "a.b-c_1", ""] {
        assert!(validate_ssh_host(ok).is_ok(), "{ok:?}");
    }
    assert_eq!(
        validate_ssh_host("my host"),
        Err(HostError::SshHostChar(' '))
    );
    assert_eq!(validate_ssh_host("a@b"), Err(HostError::SshHostChar('@')));
    assert_eq!(validate_ssh_host("a:b"), Err(HostError::SshHostChar(':')));
    assert_eq!(validate_ssh_host("a/b"), Err(HostError::SshHostChar('/')));
}

#[test]
fn the_ssh_url_comes_from_the_web_url_gh_printed_and_the_chosen_host() {
    assert_eq!(
        ssh_remote_url(
            "github.com-personal",
            "https://github.com/richardlavoura/new-project"
        ),
        Some("git@github.com-personal:richardlavoura/new-project.git".to_owned())
    );
    assert_eq!(
        ssh_remote_url("alias", "https://github.com/acme/tool.git"),
        Some("git@alias:acme/tool.git".to_owned()),
        "a .git suffix is not doubled"
    );
    assert_eq!(
        ssh_remote_url("alias", "https://github.com/acme/tool/"),
        Some("git@alias:acme/tool.git".to_owned()),
        "a trailing slash is ignored"
    );
    assert_eq!(
        ssh_remote_url("alias", "https://github.com/onlyowner"),
        None
    );
    assert_eq!(ssh_remote_url("alias", "not a url"), None);
    assert_eq!(ssh_remote_url("alias", ""), None);
}

#[test]
fn set_remote_url_rewrites_a_remote_and_a_missing_one_says_so() {
    let project = Project::new("host-seturl");
    let status = Command::new("git")
        .arg("-C")
        .arg(&project.work)
        .args(["remote", "add", "origin", "https://example.com/a.git"])
        .status()
        .unwrap();
    assert!(status.success());
    let repo = project.repo();
    repo.set_remote_url("origin", "git@alias:acme/tool.git")
        .unwrap();
    assert_eq!(
        project.remote_url().as_deref(),
        Some("git@alias:acme/tool.git")
    );

    let err = repo
        .set_remote_url("nope", "git@alias:x/y.git")
        .unwrap_err();
    assert!(
        matches!(&err, GitError::HostFailed(m) if m.contains("cannot set the URL of nope")),
        "{err:?}"
    );
    // A URL that starts with a dash is a URL, not an option.
    repo.set_remote_url("origin", "-oProxyCommand=x").unwrap();
    assert_eq!(project.remote_url().as_deref(), Some("-oProxyCommand=x"));
}
