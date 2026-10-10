//! The `git2` and subprocess adapter behind the `GitPort` traits (`ferrit_domain::port`).
//! `Repo` opens a repository and composes the capability implementations. Each
//! backend module owns one Git capability and its `Repo` forwarding methods.

pub(crate) mod blob;
pub(crate) mod branches;
pub(crate) mod commit;
pub(crate) mod diff;
pub(crate) mod exec;
pub(crate) mod gitconfig;
pub(crate) mod index;
mod init;
pub(crate) mod log;
pub(crate) mod port_impl;
pub(crate) mod process;
pub(crate) mod read;
pub(crate) mod rebase;
pub(crate) mod remotes;
pub(crate) mod stashes;
pub(crate) mod statistics;
pub(crate) mod status;

use std::path::Path;

use git2::Repository;

use ferrit_domain::Snapshot;
use ferrit_domain::error::{GitError, GitResult};
use ferrit_domain::identity::{Identity, IdentitySource};

/// A `git2` failure with the text git gave, kept as the `source` of
/// `GitError::Read` and `GitError::Open`.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct Git2Failure {
    message: String,
    #[source]
    source: git2::Error,
}

impl From<git2::Error> for Git2Failure {
    fn from(source: git2::Error) -> Self {
        Self {
            message: source.message().to_owned(),
            source,
        }
    }
}

/// A `git2` read that failed.
pub(crate) fn read_error(error: git2::Error) -> GitError {
    GitError::Read(Box::new(Git2Failure::from(error)))
}

/// The repository could not be opened.
fn open_error(error: git2::Error) -> GitError {
    GitError::Open(Box::new(Git2Failure::from(error)))
}

/// How many commits `Repo::snapshot()` reads for the Commits pane. lazygit lists every
/// commit; 1,000 covers all but the largest histories (the counter then reads `1 of
/// 1000`) while the walk stays cheap. Plain constant until the pane pages.
pub(crate) const COMMITS_LIMIT: usize = 1000;

fn config_values(config: &git2::Config, key: &str) -> Vec<String> {
    let Ok(mut entries) = config.multivar(key, None) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    while let Some(Ok(entry)) = entries.next() {
        if let Ok(value) = entry.value() {
            values.push(value.to_owned());
        }
    }
    values
}

fn unique_identities(identities: impl IntoIterator<Item = Identity>) -> Vec<Identity> {
    let mut unique = Vec::new();
    for identity in identities {
        if !unique.contains(&identity) {
            unique.push(identity);
        }
    }
    unique
}

fn config_identities(config: &git2::Config) -> Vec<Identity> {
    let names = config_values(config, "user.name");
    let emails = config_values(config, "user.email");
    unique_identities(names.into_iter().enumerate().map(|(index, name)| Identity {
        name,
        email: emails.get(index).cloned(),
    }))
}

fn global_config_identities(config: &git2::Config) -> Vec<Identity> {
    [git2::ConfigLevel::XDG, git2::ConfigLevel::Global]
        .into_iter()
        .filter_map(|level| config.open_level(level).ok())
        .flat_map(|level| config_identities(&level))
        .fold(Vec::new(), |mut unique, identity| {
            if !unique.contains(&identity) {
                unique.push(identity);
            }
            unique
        })
}

fn config_identity(config: &git2::Config) -> Option<Identity> {
    let name = config.get_string("user.name").ok()?;
    Some(Identity {
        name,
        email: config.get_string("user.email").ok(),
    })
}

/// An open repository. Wraps `git2::Repository` and hands out owned snapshots.
pub struct Repo {
    inner: Repository,
    /// Path used to discover this repository. Lets the TUI reopen an owned
    /// handle inside a refresh worker; `git2::Repository` itself stays local.
    reopen_path: std::path::PathBuf,
    /// A throwaway global config file, set by `isolate_config`. `None` in
    /// real use: git then reads the user's own files.
    config_global: Option<std::path::PathBuf>,
}

/// Abbreviated hash, the 7 hex chars `git` shows by default. Shared by
/// `status` (upstream not needed there, but commits/refs both want it).
pub(crate) fn short_hash(oid: &git2::Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

#[allow(
    clippy::same_name_method,
    reason = "the repository core forwards capability reads under their public names"
)]
impl Repo {
    /// Open the repository at or above `path`. Walks up like `git` does.
    pub fn open(path: &Path) -> GitResult<Self> {
        let reopen_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let inner = Repository::discover(path).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                GitError::NotARepository(path.to_path_buf())
            } else {
                open_error(e)
            }
        })?;
        Ok(Self {
            inner,
            reopen_path,
            config_global: None,
        })
    }

    /// Path for opening a fresh handle in a background worker.
    pub fn reopen_path(&self) -> &Path {
        &self.reopen_path
    }

    /// The repository's directory name, e.g. `ferrit`. Used in the status
    /// header (`ferrit -> main`). Falls back to `"repo"` for odd layouts.
    pub fn name(&self) -> String {
        self.inner
            .workdir()
            .and_then(|w| w.file_name())
            .or_else(|| self.inner.path().parent().and_then(|p| p.file_name()))
            .map_or_else(|| "repo".to_owned(), |n| n.to_string_lossy().into_owned())
    }

    /// Configured Git author name (`user.name`), respecting repo, global,
    /// and system config precedence.
    pub fn user_name(&self) -> Option<String> {
        self.inner.config().ok()?.get_string("user.name").ok()
    }

    /// Global choices, repository override, resolved identity, and its source.
    pub fn identity_settings(
        &self,
    ) -> (
        Vec<Identity>,
        Option<Identity>,
        Option<Identity>,
        IdentitySource,
    ) {
        let global_identities = git2::Config::open_default()
            .ok()
            .map_or_else(Vec::new, |config| global_config_identities(&config));
        let local_config = git2::Config::open(&self.inner.path().join("config")).ok();
        let repository_identity = local_config.as_ref().and_then(config_identity);
        let repository_overrides_identity = local_config.as_ref().is_some_and(|config| {
            config.get_entry("user.name").is_ok() || config.get_entry("user.email").is_ok()
        });
        let effective_identity = self
            .inner
            .config()
            .ok()
            .and_then(|config| config_identity(&config));
        let identity_source = if repository_overrides_identity {
            IdentitySource::Repository
        } else if global_identities.is_empty() {
            if effective_identity.is_some() {
                IdentitySource::System
            } else {
                IdentitySource::Unset
            }
        } else {
            IdentitySource::Global
        };
        (
            global_identities,
            repository_identity,
            effective_identity,
            identity_source,
        )
    }

    /// Re-read every wired pane in one go. Partial failure fails the whole call.
    ///
    /// `&mut self`: reading the stash list needs `&mut git2::Repository`.
    pub fn snapshot(&mut self) -> GitResult<Snapshot> {
        Ok(Snapshot {
            header: status::header(&self.inner)?,
            files: status::files(&self.inner)?,
            branches: branches::branches(&self.inner)?,
            commits: log::commits(&self.inner, COMMITS_LIMIT)?,
            stashes: stashes::stashes(&mut self.inner)?,
            remotes: remotes::remotes(&self.inner)?,
            operation: rebase::current(&self.inner),
        })
    }
}

#[cfg(test)]
mod identity_tests {
    use crate::repo::unique_identities;
    use ferrit_domain::identity::Identity;

    #[test]
    fn removes_duplicate_identity_pairs_and_preserves_first_seen_order() {
        let max = Identity {
            name: "Max Wells".to_owned(),
            email: Some("max@example.com".to_owned()),
        };
        let username = Identity {
            name: "Username".to_owned(),
            email: Some("user@example.com".to_owned()),
        };
        let distinct_email = Identity {
            name: "Max Wells".to_owned(),
            email: Some("other@example.com".to_owned()),
        };

        assert_eq!(
            unique_identities([
                max.clone(),
                username.clone(),
                max.clone(),
                distinct_email.clone(),
            ]),
            [max, username, distinct_email]
        );
    }
}
