//! Fetch, pull and push: what is asked for, and how a key decides between a
//! plain push, a force push, a push that sets the upstream, and a question.
//! The call itself is the port's; the thread it runs on is `App`'s.

use std::sync::atomic::AtomicBool;

use crate::error::GitResult;
use crate::model::{RemoteEntry, StatusHeader};
use crate::port::GitPort;

/// Which network operation is in flight or just finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOp {
    /// `git fetch`.
    Fetch,
    /// `git pull`.
    Pull,
    /// `git push`.
    Push,
    /// `gh repo create`: it has its own start, since it takes a form's fields,
    /// but it shares the slot with the other three.
    Create,
}

impl RemoteOp {
    /// The Status pane's label while it runs.
    #[must_use]
    pub const fn busy_label(self) -> &'static str {
        match self {
            Self::Fetch => "Fetching\u{2026}",
            Self::Pull => "Pulling\u{2026}",
            Self::Push => "Pushing\u{2026}",
            Self::Create => "Creating repository\u{2026}",
        }
    }

    /// The short verb attached to the checked-out branch row.
    #[must_use]
    pub const fn branch_label(self) -> &'static str {
        match self {
            Self::Fetch => "Fetching",
            Self::Pull => "Pulling",
            Self::Push => "Pushing",
            Self::Create => "Creating",
        }
    }
}

/// One network operation and how it is asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRequest {
    /// What to do.
    pub op: RemoteOp,
    /// `push -u <remote>`: the remote to set as upstream.
    pub push_upstream: Option<String>,
    /// `push -u <remote> <local>:<branch>`: the branch on the remote.
    pub upstream_branch: Option<String>,
    /// `--force-with-lease`.
    pub force_with_lease: bool,
    /// Set the upstream to the branch of the same name (`push.default=current`).
    pub set_upstream_current: bool,
}

impl RemoteRequest {
    /// A plain `op`.
    #[must_use]
    pub const fn new(op: RemoteOp) -> Self {
        Self {
            op,
            push_upstream: None,
            upstream_branch: None,
            force_with_lease: false,
            set_upstream_current: false,
        }
    }

    /// A push that sets `remote`'s `branch` as the upstream.
    #[must_use]
    pub fn push_to(remote: String, branch: String) -> Self {
        Self {
            push_upstream: Some(remote),
            upstream_branch: Some(branch),
            ..Self::new(RemoteOp::Push)
        }
    }

    /// A push with `--force-with-lease`.
    #[must_use]
    pub fn force_push() -> Self {
        Self {
            force_with_lease: true,
            ..Self::new(RemoteOp::Push)
        }
    }

    /// A push that sets the upstream to the branch of the same name.
    #[must_use]
    pub fn push_current() -> Self {
        Self {
            set_upstream_current: true,
            ..Self::new(RemoteOp::Push)
        }
    }
}

/// Make the call; `cancel` set from another thread stops it. The line git
/// answers with on success.
pub fn run(repo: &dyn GitPort, request: &RemoteRequest, cancel: &AtomicBool) -> GitResult<String> {
    match request.op {
        RemoteOp::Fetch => repo.fetch_cancellable(None, cancel),
        RemoteOp::Pull => repo.pull_cancellable(cancel),
        RemoteOp::Push => repo.push_cancellable(
            request.push_upstream.as_deref(),
            request.upstream_branch.as_deref(),
            request.force_with_lease,
            request.set_upstream_current,
            cancel,
        ),
        // Started by `App::start_create_remote`, never through here.
        RemoteOp::Create => Ok(String::new()),
    }
}

/// What `P` comes to, decided from what the Status header already knows (no
/// call to git to ask whether there is an upstream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushPlan {
    /// Push as is.
    Plain,
    /// The branch is behind (or has diverged from) its upstream: ask before
    /// pushing with a lease; the text is the question.
    ConfirmForce(String),
    /// No upstream, and `push.default=current`: push and set it.
    SetCurrent,
    /// No upstream: ask for `<remote> <branch>`, starting from this text.
    AskUpstream(String),
}

/// Decide a push. Having no upstream honours `push.default=current`,
/// otherwise proposes `origin <branch>` (or the first remote).
pub fn plan_push(
    header: &StatusHeader,
    remotes: &[RemoteEntry],
    push_default_current: bool,
) -> PushPlan {
    if header.upstream.is_some() {
        if header.behind == 0 {
            return PushPlan::Plain;
        }
        // Ahead and behind at once is what rewriting pushed commits leaves
        // (`docs/PLAN_11_REBASE.md`): say so, not just "behind".
        return PushPlan::ConfirmForce(if header.ahead > 0 {
            format!(
                "Branch has diverged from upstream (ahead {}, behind {}), as after rewriting \
                 pushed commits. Push with --force-with-lease?",
                header.ahead, header.behind
            )
        } else {
            format!(
                "Branch is behind upstream by {} commit(s). Push with --force-with-lease?",
                header.behind
            )
        });
    }
    if push_default_current {
        return PushPlan::SetCurrent;
    }
    let remote = remotes
        .iter()
        .find(|remote| remote.name == "origin")
        .or_else(|| remotes.first())
        .map_or("origin", |remote| remote.name.as_str());
    PushPlan::AskUpstream(format!("{remote} {}", header.branch))
}

/// `<remote> <branch>` as typed in the upstream prompt: exactly two words.
#[must_use]
pub fn parse_upstream(value: &str) -> Option<(&str, &str)> {
    let mut parts = value.split_whitespace();
    match (parts.next(), parts.next(), parts.next()) {
        (Some(remote), Some(branch), None) => Some((remote, branch)),
        _ => None,
    }
}
