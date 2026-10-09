//! Creating the repository on GitHub through the user's own `gh`
//! (`docs/PLAN_15_CREATE_REMOTE.md`). ferrit never holds a token: `gh` is
//! already signed in. This module validates what the user typed, builds
//! `gh`'s argument list from the validated fields only, and (see the other half
//! of the file) asks `gh` whether it is ready.
//!
//! The target is typed as `name` or `owner/name`; there is no separate owner.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::git::repo::host`.

use crate::git::process::{REMOTE_TIMEOUT, run_child};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Duration;

use crate::git::error::GitError;
use crate::git::exec;

/// Longest repository name GitHub takes.
pub(crate) const NAME_MAX: usize = 100;

/// Longest account or organisation login GitHub takes.
pub(crate) const OWNER_MAX: usize = 39;

/// Longest repository description GitHub takes.
pub(crate) const DESCRIPTION_MAX: usize = 350;

/// The program to run for `gh`: `gh` from `PATH`, unless a test hands in a
/// fake script. An injected value, not an environment variable: the crate
/// forbids `unsafe`, so a test cannot set one in-process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhProgram {
    program: OsString,
    pub(crate) timeout: Duration,
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

    pub(crate) fn program(&self) -> &OsStr {
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
    /// Installed and signed in: a repository can be created.
    Ready,
}

/// Who can see the repository `gh` creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// The default everywhere in ferrit.
    Private,
    /// Anyone.
    Public,
}

/// What to create. Fields are checked by `build_create_args`, never trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateRequest {
    /// An account or organisation; `None` is `gh`'s own default (the signed-in
    /// account).
    pub owner: Option<String>,
    /// The repository's name, without the owner.
    pub name: String,
    /// Private or public.
    pub visibility: Visibility,
    /// One line, may be empty.
    pub description: String,
}

/// Why the typed text was refused, before any process starts.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    /// The name is empty.
    #[error("the repository needs a name")]
    EmptyName,
    /// The name is longer than GitHub allows.
    #[error("the name is longer than {NAME_MAX} characters")]
    NameTooLong,
    /// The name has a character GitHub refuses.
    #[error("'{0}' is not allowed in a name: letters, digits, '-', '_' and '.'")]
    NameChar(char),
    /// The name is one GitHub reserves (`.` or `..`).
    #[error("'{0}' is not a usable name")]
    NameReserved(String),
    /// The owner before the slash is empty.
    #[error("the owner is empty: write `owner/name`, or just the name")]
    EmptyOwner,
    /// The owner is longer than GitHub allows.
    #[error("the owner is longer than {OWNER_MAX} characters")]
    OwnerTooLong,
    /// The owner has a character GitHub refuses.
    #[error("'{0}' is not allowed in an owner: letters, digits and '-'")]
    OwnerChar(char),
    /// More than one slash: only `owner/name` or `name` are understood.
    #[error("write `owner/name` or just the name, with one slash at most")]
    TooManySlashes,
    /// The SSH host alias has a character an SSH config does not take.
    #[error("'{0}' is not allowed in an SSH host: letters, digits, '.', '-' and '_'")]
    SshHostChar(char),
    /// The description has a line break.
    #[error("the description takes one line")]
    DescriptionLines,
    /// The description is longer than GitHub allows.
    #[error("the description is longer than {DESCRIPTION_MAX} characters")]
    DescriptionTooLong,
}

pub(crate) fn check_name(name: &str) -> Result<(), HostError> {
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

pub(crate) fn check_owner(owner: &str) -> Result<(), HostError> {
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

/// An SSH host as `~/.ssh/config` names it (`github.com-personal`): letters,
/// digits, `.`, `-` and `_`. Empty is accepted: it means "keep the URL `gh`
/// wrote".
pub fn validate_ssh_host(host: &str) -> Result<(), HostError> {
    match host
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')))
    {
        Some(bad) => Err(HostError::SshHostChar(bad)),
        None => Ok(()),
    }
}

/// `git@<host>:<owner>/<name>.git` for the repository `gh` printed the web URL
/// of (`https://github.com/owner/name`), over the SSH host `host`. A remote
/// written with a host alias reaches the key that alias names, which the plain
/// `github.com` does not. `None` when the web URL has no owner and name.
#[must_use]
pub fn ssh_remote_url(host: &str, web_url: &str) -> Option<String> {
    let after_scheme = web_url.split_once("://").map_or(web_url, |(_, rest)| rest);
    let path = after_scheme.split_once('/')?.1;
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let name = parts.next()?.trim_end_matches(".git");
    (!name.is_empty()).then(|| format!("git@{host}:{owner}/{name}.git"))
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
pub(crate) fn web_url(stdout: &str) -> String {
    stdout
        .lines()
        .map(str::trim)
        .rfind(|line| line.starts_with("https://") || line.starts_with("http://"))
        .unwrap_or_default()
        .to_owned()
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
