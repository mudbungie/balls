//! Git-remote `Protocol` impl for state-branch lifecycle pushes.
//!
//! bl-2148 introduced this wire for the claim path: local claim
//! commit on `balls/tasks` must land on origin before the worktree
//! is created, with non-fast-forward rejections driving a
//! fetch + field-level merge + retry. bl-2bf7 generalizes it to
//! review and close — same wire, same retry-merge loop, only
//! `post_merge`'s claim-vs-lost ownership check is event-specific.
//!
//! Wired in per event only when the corresponding
//! `require_remote_on_<event>` policy is true; otherwise the event
//! stays local-only. The propose-merge-retry primitive itself lives
//! in `crate::negotiation`; this module supplies the wire hooks and
//! `push_claim` / `push_state_for` wrappers.

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

/// `Protocol` implementation for the git-remote state-branch lifecycle
/// push. One concrete struct serves every event because the wire is
/// the same `git push origin balls/tasks` either way; the only
/// branching is `post_merge`'s claim-only ownership check.
pub struct GitRemoteClaimProtocol<'a> {
    event: Event,
    store: &'a Store,
    task_id: &'a str,
    identity: &'a str,
    state_dir: PathBuf,
}

impl<'a> GitRemoteClaimProtocol<'a> {
    pub fn new(event: Event, store: &'a Store, task_id: &'a str, identity: &'a str) -> Self {
        let state_dir = store.state_worktree_dir();
        Self { event, store, task_id, identity, state_dir }
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
            git::git_commit(&self.state_dir, "state: auto-resolve lifecycle conflicts")?;
        }
        Ok(())
    }

    fn post_merge(&mut self) -> Result<Option<SyncedClaimResult>> {
        // Only the claim event has a "lost" semantics — the
        // claim-race resolution turns the merge into a definitive
        // win/lose outcome. Review and close just retry the push
        // after the field-level merge resolves any divergence.
        if self.event != Event::Claim {
            return Ok(None);
        }
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
/// state itself — the per-event `Protocol` owns state for one
/// negotiation. Subscriptions are caller-controlled: `for_claim()` is
/// the bl-2148 shape (claim only); `for_lifecycle(events)` is the
/// bl-2bf7 generalization for review and close. The caller's policy
/// resolution (per-event `require_remote_on_*` plus non-stealth) is
/// what decides whether to subscribe at all.
pub struct GitRemoteParticipant {
    projection: Projection,
    subscriptions: Vec<Event>,
}

impl GitRemoteParticipant {
    /// Subscribe only to `claim`. Equivalent to
    /// `for_lifecycle(&[Event::Claim])`; kept as a named constructor
    /// because the bl-2148 call sites read more clearly that way.
    pub fn for_claim() -> Self {
        Self::for_lifecycle(&[Event::Claim])
    }

    /// Subscribe to the supplied lifecycle events. Failure on any
    /// subscribed event is `Required` — the rollback semantics in
    /// the lifecycle paths depend on the negotiation surfacing the
    /// failure as `Err`. Callers that want best-effort behavior
    /// should not subscribe at all (i.e. don't construct the
    /// participant for that event).
    pub fn for_lifecycle(events: &[Event]) -> Self {
        Self {
            projection: Projection::full(),
            subscriptions: events.to_vec(),
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
        // Subscribed events are always Required: the call site only
        // wires the participant when policy says the remote must
        // succeed for this transition. Unsubscribed events never
        // reach this method through `participant::run` (the
        // subscription gate short-circuits first), so the fallback
        // is just a safe answer.
        if self.subscriptions.contains(&event) {
            FailurePolicy::Required
        } else {
            FailurePolicy::BestEffort
        }
    }

    fn protocol<'a>(
        &'a self,
        event: Event,
        ctx: EventCtx<'a>,
    ) -> Option<Self::Protocol<'a>> {
        match event {
            Event::Claim | Event::Review | Event::Close => Some(
                GitRemoteClaimProtocol::new(event, ctx.store, ctx.task_id, ctx.identity),
            ),
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
    push_state_for(store, task_id, identity, Event::Claim, "claim --sync")
}

/// Push the freshly-committed state-branch transition for `event`
/// through `origin/balls/tasks`. Required-policy generalization of
/// `push_claim` for review and close: same wire, same retry-merge
/// loop, same unreachable-aborts-loud stance. The `error_prefix`
/// is folded into the `Err` message so callers don't all wrap the
/// same way.
pub fn push_state_for(
    store: &Store,
    task_id: &str,
    identity: &str,
    event: Event,
    error_prefix: &str,
) -> Result<SyncedClaimResult> {
    let state_dir = store.state_worktree_dir();
    if !git::git_fetch(&state_dir, REMOTE)? {
        return Err(BallError::Other(format!(
            "{error_prefix}: cannot reach remote `{REMOTE}` (fetch failed)"
        )));
    }
    let participant = GitRemoteParticipant::for_lifecycle(&[event]);
    let ctx = EventCtx { event, store, task_id, identity };
    participant::run_strict(&participant, event, ctx)
        .map_err(|e| BallError::Other(format!("{error_prefix}: {e}")))
}

#[cfg(test)]
#[path = "claim_sync_tests.rs"]
mod tests;
