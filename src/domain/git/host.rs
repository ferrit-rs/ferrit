//! Creating the repository on GitHub through the user's own `gh`
//! (`docs/PLAN_15_CREATE_REMOTE.md`). ferrit never holds a token: `gh` is
//! already signed in. This module validates what the user typed, builds
//! `gh`'s argument list from the validated fields only, and (see the other half
//! of the file) asks `gh` whether it is ready.
//!
//! The target is typed as `name` or `owner/name`; there is no separate owner.

use std::ffi::OsString;
use std::path::Path;

/// Longest repository name GitHub takes.
const NAME_MAX: usize = 100;
/// Longest account or organisation login GitHub takes.
const OWNER_MAX: usize = 39;
/// Longest repository description GitHub takes.
const DESCRIPTION_MAX: usize = 350;

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
