//! Lifecycle-sync negotiation primitive.
//!
//! One loop, many wire protocols. `Negotiation::run` drives a
//! `Protocol` impl through propose -> classify -> fetch_remote_view ->
//! post_merge -> retry until the wire accepts the proposal, the merge
//! short-circuits to a non-default outcome, or the retry budget is
//! exhausted. Wire-specific behavior — what counts as a conflict, how
//! the remote view is fetched, what merge means — is hidden behind
//! the trait. Failure absorption is selected at construction by
//! `FailurePolicy`.
//!
//! Why a primitive: bl-2148 wired the propose-retry loop inline in
//! the claim path, and the legacy plugin dispatcher was reinventing
//! the same shape with weaker guarantees. Collapsing both onto this
//! primitive (this ball; bl-1ea6 wires participants on top) means
//! every future participant inherits one set of semantics for retry,
//! conflict handling, and failure policy instead of growing parallel
//! ones.

use crate::error::{BallError, Result};

/// Classification a wire returns for a single propose attempt.
/// Mirrors the SPEC's `ConflictClass`.
#[derive(Debug, PartialEq, Eq)]
pub enum AttemptClass {
    /// Wire accepted the proposal.
    Ok,
    /// Wire rejected because its view advanced past ours; recoverable
    /// via fetch + merge + retry.
    Conflict,
    /// Peer is not contactable; not recoverable in this run.
    Unreachable(String),
    /// Any other wire failure not covered above.
    Other(String),
}

/// How an exhausted-retry or unreachable peer should affect the
/// caller. Per SPEC §9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailurePolicy {
    /// Failure aborts the lifecycle event. Caller sees `Err`.
    Required,
    /// Failure is absorbed; caller sees `NegotiationResult::Skipped`.
    BestEffort,
    /// Failure is staged for later human review; caller sees
    /// `NegotiationResult::Staged`. Concrete staging plumbing lands
    /// with bl-a46d; here the variant just carries the message.
    Gating,
}

/// SPEC §10 — how a successful participant outcome should be
/// committed to the state branch. Returned from
/// `Protocol::commit_policy`; composed across participants by the
/// apply-time helper in `crate::commit_policy`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitPolicy {
    /// Write a commit on the state branch, optionally with a
    /// participant-supplied message body. `None` means "use the
    /// caller's default end-of-event commit message"; this is the
    /// default behavior and reproduces today's per-op commit shape.
    Commit { message: Option<String> },
    /// Accumulate the participant's state changes within the current
    /// event dispatch and commit once at the end of the event.
    /// Multiple participants returning `Batch` with the same `tag`
    /// coalesce into a single commit referencing every batched
    /// participant.
    Batch { tag: String },
    /// Apply state to the working tree but do not write a commit on
    /// behalf of this participant. The next `Commit` outcome — or the
    /// caller's fallback end-of-event commit — picks up the change.
    /// Disallowed for `FailurePolicy::Required` participants; the
    /// apply-time helper rejects it before any state lands.
    Suppress,
}

impl Default for CommitPolicy {
    fn default() -> Self {
        CommitPolicy::Commit { message: None }
    }
}

/// A successful negotiation outcome plus the participant's chosen
/// commit policy. The `Protocol` returns these together; the dispatch
/// layer routes `commit_policy` through the apply-time composer.
#[derive(Debug, PartialEq, Eq)]
pub struct Accepted<O> {
    pub outcome: O,
    pub commit_policy: CommitPolicy,
}

/// Result of a completed negotiation, parameterized over the
/// protocol-specific success outcome.
#[derive(Debug, PartialEq, Eq)]
pub enum NegotiationResult<O> {
    /// Wire accepted (possibly after merges) or `post_merge`
    /// short-circuited with a definitive outcome.
    Ok(Accepted<O>),
    /// Wire failure absorbed by `FailurePolicy::BestEffort`.
    Skipped(String),
    /// Wire failure absorbed by `FailurePolicy::Gating`.
    Staged(String),
}

/// Wire-specific hooks the negotiation loop drives. Implementors own
/// their state; the loop just sequences calls.
pub trait Protocol {
    /// Value returned to the caller on success.
    type Outcome;

    /// Attempt to publish the proposal once and classify the result.
    fn propose(&mut self) -> Result<AttemptClass>;

    /// Pull the peer's view in and merge it into local working state.
    /// Called once per `Conflict` before the loop retries.
    fn fetch_remote_view(&mut self) -> Result<()>;

    /// After a successful `fetch_remote_view`, decide whether the
    /// merge changed our footing enough to abandon the retry. Return
    /// `Ok(Some(outcome))` to short-circuit (e.g. claim race lost),
    /// `Ok(None)` to retry. Default: always retry.
    fn post_merge(&mut self) -> Result<Option<Self::Outcome>> {
        Ok(None)
    }

    /// Build the success outcome on a clean push.
    fn pushed(&mut self) -> Self::Outcome;

    /// Maximum propose attempts before the loop gives up.
    fn retry_budget(&self) -> usize;

    /// The commit policy to attach to a successful outcome. Default is
    /// `CommitPolicy::Commit { message: None }` — i.e. fold this
    /// participant's state change into the caller's default
    /// end-of-event commit. Native plugins (bl-8b71) override to
    /// supply custom messages, batch tags, or suppression. SPEC §10.
    fn commit_policy(&self) -> CommitPolicy {
        CommitPolicy::default()
    }
}

/// The negotiation loop. Construct with a protocol and a failure
/// policy; call `run` once.
pub struct Negotiation<P: Protocol> {
    protocol: P,
    failure_policy: FailurePolicy,
}

impl<P: Protocol> Negotiation<P> {
    pub fn new(protocol: P, failure_policy: FailurePolicy) -> Self {
        Self { protocol, failure_policy }
    }

    /// Drive the propose-merge-retry loop until completion.
    pub fn run(mut self) -> Result<NegotiationResult<P::Outcome>> {
        let budget = self.protocol.retry_budget();
        for _ in 0..budget {
            let class = self.protocol.propose()?;
            if class == AttemptClass::Ok {
                let outcome = self.protocol.pushed();
                return Ok(NegotiationResult::Ok(self.accept(outcome)));
            }
            if let AttemptClass::Unreachable(s) | AttemptClass::Other(s) = class {
                return self.classify_failure(s);
            }
            // Conflict: fetch + merge, then re-check whether our
            // proposal still stands.
            self.protocol.fetch_remote_view()?;
            if let Some(outcome) = self.protocol.post_merge()? {
                return Ok(NegotiationResult::Ok(self.accept(outcome)));
            }
        }
        self.classify_failure(format!("gave up after {budget} attempts; remote keeps advancing"))
    }

    /// Run and unwrap the `Ok` variant; absorb-policy variants are
    /// surfaced as `Err`. Convenience for `Required`-policy callers
    /// that structurally cannot produce `Skipped`/`Staged`. The
    /// participant's `CommitPolicy` is dropped here — strict callers
    /// (today: claim path) commit before they call into the
    /// negotiation, so the policy has nothing to schedule.
    pub fn run_strict(self) -> Result<P::Outcome> {
        match self.run()? {
            NegotiationResult::Ok(Accepted { outcome, .. }) => Ok(outcome),
            NegotiationResult::Skipped(s) | NegotiationResult::Staged(s) => {
                Err(BallError::Other(s))
            }
        }
    }

    fn accept(&self, outcome: P::Outcome) -> Accepted<P::Outcome> {
        Accepted {
            outcome,
            commit_policy: self.protocol.commit_policy(),
        }
    }

    fn classify_failure(&self, msg: String) -> Result<NegotiationResult<P::Outcome>> {
        match self.failure_policy {
            FailurePolicy::Required => Err(BallError::Other(msg)),
            FailurePolicy::BestEffort => Ok(NegotiationResult::Skipped(msg)),
            FailurePolicy::Gating => Ok(NegotiationResult::Staged(msg)),
        }
    }
}

#[cfg(test)]
#[path = "negotiation_tests.rs"]
mod tests;
