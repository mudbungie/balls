//! Git-remote `Protocol` impl for the claim path. Closes the
//! claim-race window between offline agents by requiring the local
//! claim commit on `balls/tasks` to land on the remote before the
//! worktree is created. Race resolution: git's native CAS
//! (non-fast-forward push rejection) drives a fetch +
//! `resolve_conflict()` merge; whichever agent's `claimed_by`
//! survives the field-level merge wins.
//!
//! Wired in only when `ClaimPolicy::require_remote` is true. With it
//! off, claims stay local-only — preserving the offline-and-solo
//! workflow.
//!
//! The propose-merge-retry loop itself lives in
//! `crate::negotiation`; this module supplies the wire-specific
//! hooks and a thin `push_claim` wrapper that the worktree path
//! calls.

use crate::error::{BallError, Result};
use crate::git;
use crate::negotiation::{AttemptClass, FailurePolicy, Protocol};
use crate::participant::{self, Event, EventCtx, Participant, Projection};
use crate::store::Store;
use std::path::{Path, PathBuf};
use std::process::Output;

const REMOTE: &str = "origin";
const STATE_BRANCH: &str = "balls/tasks";
const MAX_RETRIES: usize = 5;

/// Run `git push origin balls/tasks` from `dir` and classify the
/// outcome. Spawn failures propagate as Err — they're catastrophic
/// (no git on PATH) rather than a remote-state condition.
pub fn push_state_classified(dir: &Path) -> Result<AttemptClass> {
    let out = git::clean_git_command(dir)
        .args(["push", REMOTE, STATE_BRANCH])
        .output()?;
    Ok(classify_push_output(&out))
}

fn classify_push_output(out: &Output) -> AttemptClass {
    if out.status.success() {
        return AttemptClass::Ok;
    }
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let l = stderr.to_lowercase();
    if l.contains("rejected")
        && (l.contains("non-fast-forward")
            || l.contains("fetch first")
            || l.contains("[rejected]"))
    {
        return AttemptClass::Conflict;
    }
    if is_unreachable(&l) {
        return AttemptClass::Unreachable(stderr);
    }
    AttemptClass::Other(stderr)
}

fn is_unreachable(stderr_lower: &str) -> bool {
    const UNREACHABLE_MARKERS: &[&str] = &[
        "could not resolve",
        "could not read from remote",
        "connection refused",
        "connection timed out",
        "connection reset",
        "repository not found",
        "permission denied",
        "does not appear to be a git repository",
        "unable to access",
        "host key verification failed",
        "no such host",
        "network is unreachable",
    ];
    UNREACHABLE_MARKERS.iter().any(|m| stderr_lower.contains(m))
}

/// Outcome of the claim-sync negotiation.
#[derive(Debug, PartialEq, Eq)]
pub enum SyncedClaimResult {
    /// Our claim landed on the remote.
    Pushed,
    /// Another agent got there first; their claim is now reflected in
    /// the local task file (via the auto-resolved merge). The caller
    /// must NOT create a worktree.
    Lost { winner: String },
}

/// `Protocol` implementation for the git-remote state-branch claim
/// push. Owns the state-branch worktree path, the task id, and the
/// caller's identity; the negotiation primitive sequences the hooks.
pub struct GitRemoteClaimProtocol<'a> {
    store: &'a Store,
    task_id: &'a str,
    identity: &'a str,
    state_dir: PathBuf,
}

impl<'a> GitRemoteClaimProtocol<'a> {
    pub fn new(store: &'a Store, task_id: &'a str, identity: &'a str) -> Self {
        let state_dir = store.state_worktree_dir();
        Self { store, task_id, identity, state_dir }
    }
}

impl Protocol for GitRemoteClaimProtocol<'_> {
    type Outcome = SyncedClaimResult;

    fn propose(&mut self) -> Result<AttemptClass> {
        push_state_classified(&self.state_dir)
    }

    fn fetch_remote_view(&mut self) -> Result<()> {
        let _ = git::git_fetch(&self.state_dir, REMOTE);
        let merge = git::git_merge(&self.state_dir, &format!("{REMOTE}/{STATE_BRANCH}"))?;
        if matches!(merge, git::MergeResult::Conflict) {
            crate::sync_resolve::auto_resolve_task_conflicts(&self.state_dir)?;
            git::git_commit(&self.state_dir, "state: auto-resolve claim conflicts")?;
        }
        Ok(())
    }

    fn post_merge(&mut self) -> Result<Option<SyncedClaimResult>> {
        let claimer = self.store.load_task(self.task_id)?.claimed_by;
        if claimer.as_deref() == Some(self.identity) {
            return Ok(None);
        }
        let winner = claimer.unwrap_or_else(|| "(unknown)".into());
        // Best-effort post-merge push so the remote sees the resolved
        // state. Failure here doesn't change the outcome — we already
        // know we lost.
        let _ = push_state_classified(&self.state_dir);
        Ok(Some(SyncedClaimResult::Lost { winner }))
    }

    fn pushed(&mut self) -> SyncedClaimResult {
        SyncedClaimResult::Pushed
    }

    fn retry_budget(&self) -> usize {
        MAX_RETRIES
    }
}

/// The git origin remote as a SPEC §5 `Participant`. Carries no wire
/// state itself — the per-event `Protocol` (today: claim only) owns
/// state for one negotiation. Subscriptions reflect what's wired in
/// this ball; review/close arrive with bl-2bf7, sync with the
/// rerouting of the standalone sync command.
pub struct GitRemoteParticipant {
    projection: Projection,
    subscriptions: Vec<Event>,
}

impl GitRemoteParticipant {
    /// The git-remote subscribes to `claim` whenever the caller
    /// dispatches it; the caller's policy resolution (require-remote
    /// and non-stealth) decides whether to dispatch. Failure on claim
    /// is `Required` — the rollback semantics in `worktree.rs` depend
    /// on the negotiation surfacing the failure as `Err`.
    pub fn for_claim() -> Self {
        Self {
            projection: Projection::full(),
            subscriptions: vec![Event::Claim],
        }
    }
}

impl Default for GitRemoteParticipant {
    fn default() -> Self {
        Self::for_claim()
    }
}

impl Participant for GitRemoteParticipant {
    type Outcome = SyncedClaimResult;
    type Protocol<'a>
        = GitRemoteClaimProtocol<'a>
    where
        Self: 'a;

    fn name(&self) -> &'static str {
        "git-remote"
    }

    fn subscriptions(&self) -> &[Event] {
        &self.subscriptions
    }

    fn projection(&self) -> &Projection {
        &self.projection
    }

    fn failure_policy(&self, event: Event) -> FailurePolicy {
        match event {
            Event::Claim => FailurePolicy::Required,
            _ => FailurePolicy::BestEffort,
        }
    }

    fn protocol<'a>(
        &'a self,
        event: Event,
        ctx: EventCtx<'a>,
    ) -> Option<Self::Protocol<'a>> {
        match event {
            Event::Claim => Some(GitRemoteClaimProtocol::new(
                ctx.store,
                ctx.task_id,
                ctx.identity,
            )),
            _ => None,
        }
    }
}

/// Push the freshly-committed claim through `origin/balls/tasks`.
/// Caller has already (a) committed the claim locally on the state
/// branch and (b) released no locks that the merge step needs. The
/// negotiation runs through the SPEC §5 `Participant` surface so the
/// claim path shares one set of semantics with future participants.
pub fn push_claim(
    store: &Store,
    task_id: &str,
    identity: &str,
) -> Result<SyncedClaimResult> {
    let state_dir = store.state_worktree_dir();
    if !git::git_fetch(&state_dir, REMOTE)? {
        return Err(BallError::Other(format!(
            "claim --sync: cannot reach remote `{REMOTE}` (fetch failed)"
        )));
    }
    let participant = GitRemoteParticipant::for_claim();
    let ctx = EventCtx { event: Event::Claim, store, task_id, identity };
    participant::run_strict(&participant, Event::Claim, ctx)
        .map_err(|e| BallError::Other(format!("claim --sync: {e}")))
}

#[cfg(test)]
#[path = "claim_sync_tests.rs"]
mod tests;
