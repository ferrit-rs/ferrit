//! Creating the repository on GitHub through the user's own `gh`
//! (`docs/PLAN_15_CREATE_REMOTE.md`). ferrit never holds a token: `gh` is
//! already signed in. This module validates what the user typed, builds
//! `gh`'s argument list from the validated fields only, and (see the other half
//! of the file) asks `gh` whether it is ready.
//!
//! The target is typed as `name` or `owner/name`; there is no separate owner.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use git2::Repository;

use crate::domain::git::diff::workdir;
use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;
use crate::domain::git::remote::{REMOTE_TIMEOUT, combined_output, run_child};

/// Longest repository name GitHub takes.
const NAME_MAX: usize = 100;
/// Longest account or organisation login GitHub takes.
const OWNER_MAX: usize = 39;
/// Longest repository description GitHub takes.
const DESCRIPTION_MAX: usize = 350;

/// The program to run for `gh`: `gh` from `PATH`, unless a test hands in a
/// fake script. An injected value, not an environment variable: the crate
/// forbids `unsafe`, so a test cannot set one in-process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhProgram {
    program: OsString,
    timeout: Duration,
}

impl Default for GhProgram {
    fn default() -> Self {
        Self::new("gh")
    }
}

impl GhProgram {
    /// Run this program, with git's network timeout.
    pub fn new<P: Into<OsString>>(program: P) -> Self {
        Self {
            program: program.into(),
            timeout: REMOTE_TIMEOUT,
        }
    }

    /// The same program with another timeout (a test makes it short).
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn program(&self) -> &OsStr {
        &self.program
    }
}

/// Whether `gh` can create a repository right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhStatus {
    /// Not installed, or it does not run.
    Missing,
    /// Installed, no account: the user runs `gh auth login` in a shell.
    SignedOut,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// The default everywhere in ferrit.
    Private,
    Public,
}

/// What to create. Fields are checked by `build_create_args`, never trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRequest {
    /// An account or organisation; `None` is `gh`'s own default (the signed-in
    /// account).
    pub owner: Option<String>,
    pub name: String,
    pub visibility: Visibility,
    /// One line, may be empty.
    pub description: String,
}

/// Why the typed text was refused, before any process starts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("the repository needs a name")]
    EmptyName,
    #[error("the name is longer than {NAME_MAX} characters")]
    NameTooLong,
    #[error("'{0}' is not allowed in a name: letters, digits, '-', '_' and '.'")]
    NameChar(char),
    #[error("'{0}' is not a usable name")]
    NameReserved(String),
    #[error("the owner is empty: write `owner/name`, or just the name")]
    EmptyOwner,
    #[error("the owner is longer than {OWNER_MAX} characters")]
    OwnerTooLong,
    #[error("'{0}' is not allowed in an owner: letters, digits and '-'")]
    OwnerChar(char),
    #[error("write `owner/name` or just the name, with one slash at most")]
    TooManySlashes,
    #[error("the description takes one line")]
    DescriptionLines,
    #[error("the description is longer than {DESCRIPTION_MAX} characters")]
    DescriptionTooLong,
}

fn check_name(name: &str) -> Result<(), HostError> {
    if name.is_empty() {
        return Err(HostError::EmptyName);
    }
    if name.chars().count() > NAME_MAX {
        return Err(HostError::NameTooLong);
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    {
        return Err(HostError::NameChar(bad));
    }
    if name == "." || name == ".." {
        return Err(HostError::NameReserved(name.to_owned()));
    }
    Ok(())
}

fn check_owner(owner: &str) -> Result<(), HostError> {
    if owner.is_empty() {
        return Err(HostError::EmptyOwner);
    }
    if owner.chars().count() > OWNER_MAX {
        return Err(HostError::OwnerTooLong);
    }
    match owner
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-'))
    {
        Some(bad) => Err(HostError::OwnerChar(bad)),
        None => Ok(()),
    }
}

/// `name` or `owner/name`, as typed in the form, checked and split.
pub fn parse_target(text: &str) -> Result<(Option<String>, String), HostError> {
    let text = text.trim();
    let mut parts = text.split('/');
    let (first, second, extra) = (parts.next(), parts.next(), parts.next());
    if extra.is_some() {
        return Err(HostError::TooManySlashes);
    }
    let (owner, name) = match (first, second) {
        (Some(owner), Some(name)) => {
            check_owner(owner)?;
            (Some(owner.to_owned()), name)
        },
        (name, None) => (None, name.unwrap_or_default()),
        (None, Some(_)) => (None, ""),
    };
    check_name(name)?;
    Ok((owner, name.to_owned()))
}

/// One line, at most 350 characters.
pub fn validate_description(description: &str) -> Result<(), HostError> {
    if description.contains(['\n', '\r']) {
        return Err(HostError::DescriptionLines);
    }
    if description.chars().count() > DESCRIPTION_MAX {
        return Err(HostError::DescriptionTooLong);
    }
    Ok(())
}

/// A folder name turned into a usable default repository name: anything GitHub
/// refuses becomes `-`, runs of them collapse, and the ends are trimmed. An
/// empty or reserved result falls back to `repo`.
#[must_use]
pub fn sanitize_name(folder: &str) -> String {
    let mut out = String::new();
    for c in folder.trim().chars() {
        let keep = c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
        if keep {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let name: String = out
        .trim_matches('-')
        .chars()
        .take(NAME_MAX)
        .collect::<String>();
    if name.is_empty() || name == "." || name == ".." {
        "repo".to_owned()
    } else {
        name
    }
}

/// The arguments of `gh repo create`, from validated fields only. It never
/// passes `--push` (ferrit pushes itself, through its own credential popup),
/// `--add-readme`, `--gitignore` or `--license`: the local history is what gets
/// published, unchanged. `gh` writes the `origin` remote itself.
pub fn build_create_args(req: &CreateRequest, workdir: &Path) -> Result<Vec<OsString>, HostError> {
    check_name(&req.name)?;
    if let Some(owner) = &req.owner {
        check_owner(owner)?;
    }
    validate_description(&req.description)?;
    let target = match &req.owner {
        Some(owner) => format!("{owner}/{}", req.name),
        None => req.name.clone(),
    };
    let mut args: Vec<OsString> = vec!["repo".into(), "create".into(), target.into()];
    args.push(
        match req.visibility {
            Visibility::Private => "--private",
            Visibility::Public => "--public",
        }
        .into(),
    );
    args.extend(["--source".into(), workdir.as_os_str().to_owned()]);
    args.extend(["--remote".into(), "origin".into()]);
    if !req.description.is_empty() {
        args.extend(["--description".into(), req.description.clone().into()]);
    }
    Ok(args)
}

/// Run `gh <args>` and say whether it exited 0. A program that cannot be
/// started, or that outlasts the timeout, counts as a failure. Both calls are
/// recorded in the command log.
fn gh_succeeds(gh: &GhProgram, args: &[&str]) -> bool {
    let mut cmd = exec::program(gh.program());
    cmd.args(args);
    run_child(cmd, "gh", gh.timeout, None, &GitError::HostFailed)
        .is_ok_and(|out| out.status.success())
}

/// `gh --version`, then `gh auth status`. It reaches the network, so the app
/// calls it from a worker, never from the UI thread. `gh auth status` also
/// fails when the network is down: that reads as signed out, and `gh auth
/// login` is then the thing to try either way.
#[must_use]
pub fn gh_status(gh: &GhProgram) -> GhStatus {
    if !gh_succeeds(gh, &["--version"]) {
        GhStatus::Missing
    } else if gh_succeeds(gh, &["auth", "status"]) {
        GhStatus::Ready
    } else {
        GhStatus::SignedOut
    }
}

/// The repository `gh` made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedRepo {
    /// `https://github.com/owner/name`, as `gh` printed it.
    pub web_url: String,
}

/// The line `gh repo create` prints on success is the repository's web URL.
fn web_url(stdout: &str) -> String {
    stdout
        .lines()
        .map(str::trim)
        .rfind(|line| line.starts_with("https://") || line.starts_with("http://"))
        .unwrap_or_default()
        .to_owned()
}

/// `gh repo create <target> --private|--public --source <workdir> --remote
/// origin [--description …]`, without `--push`. Refuses, before running
/// anything, a target that fails validation or a repository that already has
/// an `origin` (`gh` would fail on it halfway). `gh`'s own refusal (a name
/// taken, no right to create there) comes back as its message. Nothing is
/// configured locally unless `gh` succeeds: it adds the remote itself, last.
pub(super) fn create_repo(
    repo: &Repository,
    gh: &GhProgram,
    req: &CreateRequest,
    cancel: &AtomicBool,
) -> GitResult<CreatedRepo> {
    let workdir = workdir(repo)?;
    let args = build_create_args(req, workdir).map_err(|e| GitError::HostFailed(e.to_string()))?;
    if repo.find_remote("origin").is_ok() {
        return Err(GitError::HostFailed(
            "a remote called origin already exists".to_owned(),
        ));
    }
    let mut cmd = exec::program(gh.program());
    cmd.args(args);
    let out = run_child(cmd, "gh", gh.timeout, Some(cancel), &GitError::HostFailed)?;
    if out.status.success() {
        Ok(CreatedRepo {
            web_url: web_url(&String::from_utf8_lossy(&out.stdout)),
        })
    } else {
        Err(GitError::HostFailed(combined_output(&out)))
    }
}
