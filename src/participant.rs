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
use crate::negotiation::{FailurePolicy, Negotiation, NegotiationResult, Protocol};
use crate::store::Store;
use std::collections::BTreeSet;

/// SPEC §6 — discrete state transitions `bl` runs against a task. The
/// `Drop` event is intentionally absent (SPEC §6): drop is a local
/// release with no durable change to negotiate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Event {
    Claim,
    Review,
    Close,
    Update,
    Sync,
}

/// SPEC §3 — canonical Task field set used by `Projection`. Mirrors
/// the public fields of `task::Task`. `External` is the whole
/// `external` map; per-plugin slices are declared via
/// `Projection::external_prefixes` so two plugins claiming
/// `external.jira.*` and `external.linear.*` don't appear to collide
/// at the field level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    Title,
    Type,
    Priority,
    Status,
    Parent,
    DependsOn,
    Description,
    Tags,
    Notes,
    Links,
    ClaimedBy,
    Branch,
    ClosedAt,
    UpdatedAt,
    ClosedChildren,
    External,
    SyncedAt,
    DeliveredIn,
}

impl Field {
    /// Every canonical field. The git-remote participant owns this
    /// set; plugins typically read it.
    pub fn all() -> BTreeSet<Field> {
        [
            Field::Title,
            Field::Type,
            Field::Priority,
            Field::Status,
            Field::Parent,
            Field::DependsOn,
            Field::Description,
            Field::Tags,
            Field::Notes,
            Field::Links,
            Field::ClaimedBy,
            Field::Branch,
            Field::ClosedAt,
            Field::UpdatedAt,
            Field::ClosedChildren,
            Field::External,
            Field::SyncedAt,
            Field::DeliveredIn,
        ]
        .into_iter()
        .collect()
    }
}

/// SPEC §5 — what a participant owns and reads. Disjoint owners
/// compose; overlapping owners on the same event require an explicit
/// merge function (out of scope here — see bl-b1dd, bl-8b71). External
/// slices are declared by prefix, e.g. `jira` -> `external.jira.*`.
/// That prefix-level disjointness is what makes "closed in git, open
/// in Jira" mergeable without a merge function in this ball.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Projection {
    pub owns: BTreeSet<Field>,
    pub reads: BTreeSet<Field>,
    pub external_prefixes: BTreeSet<String>,
}

impl Projection {
    /// Git-remote shape: own every canonical field, read nothing extra.
    pub fn full() -> Self {
        Self {
            owns: Field::all(),
            reads: BTreeSet::new(),
            external_prefixes: BTreeSet::new(),
        }
    }

    /// Plugin shape: own only `external.<name>.*`; read everything
    /// canonical. Used by the legacy shim (bl-b1dd) and as the default
    /// projection for native plugins until they declare richer ones.
    pub fn external_only(prefix: impl Into<String>) -> Self {
        let mut external_prefixes = BTreeSet::new();
        external_prefixes.insert(prefix.into());
        Self {
            owns: BTreeSet::new(),
            reads: Field::all(),
            external_prefixes,
        }
    }

    /// Two projections overlap if they both own the same canonical
    /// field, or the same `external.<prefix>`. SPEC §5 demands an
    /// explicit merge for overlap; until that lands (bl-8b71),
    /// callers reject overlaps at registration time.
    pub fn overlaps(&self, other: &Projection) -> bool {
        self.owns.intersection(&other.owns).next().is_some()
            || self
                .external_prefixes
                .intersection(&other.external_prefixes)
                .next()
                .is_some()
    }
}

/// SPEC §3 — context for one (event, participant) negotiation.
/// Borrowed; the negotiation primitive doesn't hold state beyond the
/// protocol it builds from this.
pub struct EventCtx<'a> {
    pub event: Event,
    pub store: &'a Store,
    pub task_id: &'a str,
    pub identity: &'a str,
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

/// Strict variant of [`run`] for callers whose failure policy is
/// structurally `Required`: collapses `Skipped` and `Staged` into
/// `Err`. Mirrors `Negotiation::run_strict` one level up. The
/// claim-sync path uses this so the not-subscribed and absorbed-
/// failure variants surface as the same error shape as a wire
/// failure (the caller already needs to roll back the local commit).
pub fn run_strict<P: Participant>(
    participant: &P,
    event: Event,
    ctx: EventCtx<'_>,
) -> Result<P::Outcome> {
    match run(participant, event, ctx)? {
        NegotiationResult::Ok(o) => Ok(o),
        NegotiationResult::Skipped(s) | NegotiationResult::Staged(s) => {
            Err(BallError::Other(s))
        }
    }
}

#[cfg(test)]
#[path = "participant_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "participant_projection_tests.rs"]
mod projection_tests;
