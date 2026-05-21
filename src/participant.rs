//! Lifecycle-sync participants. SPEC §3, §5–§7.
//!
//! A `Participant` is a remote-or-external system that takes part in
//! negotiating one or more lifecycle events. The git origin remote is
//! a participant. The legacy plugin shim (bl-b1dd) and any future
//! native plugins will be participants too. This module defines the
//! trait surface and a `run` helper that drives one through the
//! bl-eae4 negotiation primitive. The git-remote reference impl
//! lives in `claim_sync.rs` next to its wire `Protocol`.
//!
//! Why land the trait now: SPEC §14's migration plan puts the trait
//! before the legacy shim and config schema. Rerouting the existing
//! claim path through it shakes the trait shape out against a real
//! wire before plugins inherit it. Plugin dispatch in
//! `commands/lifecycle.rs` is intentionally untouched in this ball.
//!
//! What's NOT here yet: the merge function on `Projection` (lands
//! with bl-b1dd / bl-8b71 once a second participant exists to merge
//! against), `CommitPolicy` on outcomes (bl-4e7d), config-driven
//! subscription resolution (bl-50c5), and review/close events on
//! participants (bl-2bf7). Those are intentionally out of scope.

use crate::error::{BallError, Result};
use crate::negotiation::{Accepted, FailurePolicy, Negotiation, NegotiationResult, Protocol};
use crate::store::Store;
use crate::task::Task;
use serde::{Deserialize, Serialize};

// `Field` and `Projection` live in `participant_projection` to keep
// this file under the 300-line cap; re-exported so existing
// `crate::participant::{Field, Projection}` paths still resolve.
pub use crate::participant_projection::{Field, Projection};

/// SPEC §6 — discrete state transitions `bl` runs against a task.
///
/// `Create` (SPEC §6.1) is a first-class, describe-gated event: task
/// birth, distinct from `Update`. `Drop` (SPEC §6.2) is observe-only:
/// a participant may be notified that a claim was released, but drop
/// changes nothing to negotiate and an observer can never block it.
///
/// Serializes to lowercase strings (`"claim"`, `"review"`, ...) so it
/// can sit as a JSON object key in `.balls/config.json` participant
/// subscription maps. New variants are additive per §13: an older
/// `bl` meeting one in a describe response drops that one
/// subscription (bl-1b07) rather than failing the handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    Claim,
    Review,
    Close,
    Update,
    Sync,
    Create,
    Drop,
}

/// SPEC §3 — context for one (event, participant) negotiation.
/// Borrowed. The push-path keys below are populated via
/// [`EventCtx::with_context`]; other callers keep the `new` defaults.
pub struct EventCtx<'a> {
    pub event: Event,
    pub store: &'a Store,
    pub task_id: &'a str,
    pub identity: &'a str,
    /// The post-image Task the command holds. A `close` archived the
    /// task file before dispatch, so the protocol must negotiate on
    /// this in-hand image rather than re-loading from the store.
    pub post: Option<&'a Task>,
    /// SPEC §5.1 — pre-image (diff basis); `None` on `create`.
    pub task_before: Option<&'a Task>,
    /// SPEC §5.1 — state-branch sha of this event's commit.
    pub commit: Option<&'a str>,
    /// SPEC §11/§5.1 — per-invocation override tokens that applied.
    pub overrides: &'a [String],
}

impl<'a> EventCtx<'a> {
    /// Bare context — the four always-present fields, push-path keys
    /// defaulted off (non-push callers use this).
    pub fn new(
        event: Event,
        store: &'a Store,
        task_id: &'a str,
        identity: &'a str,
    ) -> Self {
        Self {
            event,
            store,
            task_id,
            identity,
            post: None,
            task_before: None,
            commit: None,
            overrides: &[],
        }
    }

    /// Layer the push-path context (post-image + SPEC §5.1 keys) onto
    /// a bare context.
    #[must_use]
    pub fn with_context(
        mut self,
        post: Option<&'a Task>,
        task_before: Option<&'a Task>,
        commit: Option<&'a str>,
        overrides: &'a [String],
    ) -> Self {
        self.post = post;
        self.task_before = task_before;
        self.commit = commit;
        self.overrides = overrides;
        self
    }
}

/// SPEC §5 — the participant contract.
///
/// Generic over the wire protocol's success outcome. Participants
/// keep their typed payload (the git-remote claim returns
/// `SyncedClaimResult`; plugin participants will return their own
/// shape) instead of being forced through a union enum. A future
/// dispatch loop over heterogeneous participants will type-erase via
/// `CommitPolicy`-bearing outcome metadata once bl-4e7d lands.
///
/// `protocol` returns `Some` for events the participant subscribes to
/// and `None` otherwise. The `subscriptions` slice is the source of
/// truth for the subscription set; `protocol` returning `None` for a
/// listed event is a programmer error.
pub trait Participant {
    type Outcome;
    type Protocol<'a>: Protocol<Outcome = Self::Outcome>
    where
        Self: 'a;

    fn name(&self) -> &str;
    fn subscriptions(&self) -> &[Event];
    fn projection(&self) -> &Projection;
    fn failure_policy(&self, event: Event) -> FailurePolicy;
    fn protocol<'a>(
        &'a self,
        event: Event,
        ctx: EventCtx<'a>,
    ) -> Option<Self::Protocol<'a>>;
}

/// Drive a participant through one event. Returns `Skipped` without
/// touching the wire when the participant doesn't subscribe — that
/// way callers iterating over a registry can dispatch unconditionally
/// and let each participant's declared subscription set decide.
pub fn run<P: Participant>(
    participant: &P,
    event: Event,
    ctx: EventCtx<'_>,
) -> Result<NegotiationResult<P::Outcome>> {
    if !participant.subscriptions().contains(&event) {
        return Ok(NegotiationResult::Skipped(format!(
            "{} does not subscribe to {event:?}",
            participant.name()
        )));
    }
    let policy = participant.failure_policy(event);
    let Some(protocol) = participant.protocol(event, ctx) else {
        return Ok(NegotiationResult::Skipped(format!(
            "{} returned no protocol for {event:?}",
            participant.name()
        )));
    };
    Negotiation::new(protocol, policy).run()
}

/// Strict variant of [`run`] for structurally-`Required` callers:
/// collapses `Skipped`/`Staged` into `Err` (mirrors
/// `Negotiation::run_strict`). The claim-sync path uses this so every
/// absorbed variant surfaces as one error shape for its rollback.
pub fn run_strict<P: Participant>(
    participant: &P,
    event: Event,
    ctx: EventCtx<'_>,
) -> Result<P::Outcome> {
    match run(participant, event, ctx)? {
        NegotiationResult::Ok(Accepted { outcome, .. }) => Ok(outcome),
        NegotiationResult::Skipped(s) | NegotiationResult::Staged(s) => {
            Err(BallError::Other(s))
        }
    }
}

#[cfg(test)]
#[path = "participant_tests.rs"]
mod tests;
