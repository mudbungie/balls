//! Unit coverage for the participant trait surface. A `FakeParticipant`
//! wires the wire-side attempt outcome from closures; the negotiation
//! primitive itself is exercised in `negotiation_tests.rs`, so we
//! focus on the Participant -> Negotiation handoff and metadata helpers.

use super::*;
use crate::error::Result;
use crate::negotiation::{AttemptClass, FailurePolicy, NegotiationResult, Protocol};
use crate::store::Store;
use std::cell::RefCell;

/// Test-only participant. Owns the wire's "next attempt" closure so
/// tests can simulate Ok / Conflict / Unreachable / Other without
/// bringing up a real protocol.
struct FakeParticipant {
    name: &'static str,
    subscriptions: Vec<Event>,
    projection: Projection,
    policy: FailurePolicy,
    attempt: RefCell<Box<dyn FnMut() -> Result<AttemptClass>>>,
    budget: usize,
}

impl FakeParticipant {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            subscriptions: vec![Event::Claim],
            projection: Projection::full(),
            policy: FailurePolicy::Required,
            attempt: RefCell::new(Box::new(|| Ok(AttemptClass::Ok))),
            budget: 3,
        }
    }
    fn with_subs(mut self, s: Vec<Event>) -> Self {
        self.subscriptions = s;
        self
    }
    fn with_policy(mut self, p: FailurePolicy) -> Self {
        self.policy = p;
        self
    }
    fn with_attempt(mut self, f: Box<dyn FnMut() -> Result<AttemptClass>>) -> Self {
        self.attempt = RefCell::new(f);
        self
    }
    fn with_projection(mut self, p: Projection) -> Self {
        self.projection = p;
        self
    }
}

struct FakeProtocol<'a> {
    attempt: &'a RefCell<Box<dyn FnMut() -> Result<AttemptClass>>>,
    budget: usize,
}

impl Protocol for FakeProtocol<'_> {
    type Outcome = &'static str;
    fn propose(&mut self) -> Result<AttemptClass> {
        (self.attempt.borrow_mut())()
    }
    fn fetch_remote_view(&mut self) -> Result<()> {
        Ok(())
    }
    fn pushed(&mut self) -> Self::Outcome {
        "ok"
    }
    fn retry_budget(&self) -> usize {
        self.budget
    }
}

impl Participant for FakeParticipant {
    type Outcome = &'static str;
    type Protocol<'a>
        = FakeProtocol<'a>
    where
        Self: 'a;

    fn name(&self) -> &str {
        self.name
    }
    fn subscriptions(&self) -> &[Event] {
        &self.subscriptions
    }
    fn projection(&self) -> &Projection {
        &self.projection
    }
    fn failure_policy(&self, _event: Event) -> FailurePolicy {
        self.policy
    }
    fn protocol<'a>(
        &'a self,
        _event: Event,
        _ctx: EventCtx<'a>,
    ) -> Option<Self::Protocol<'a>> {
        Some(FakeProtocol { attempt: &self.attempt, budget: self.budget })
    }
}

fn ctx(store: &Store) -> EventCtx<'_> {
    EventCtx { event: Event::Claim, store, task_id: "bl-7e57", identity: "alice" }
}

fn stealth_store() -> (tempfile::TempDir, Store) {
    let td = tempfile::tempdir().unwrap();
    let tasks_dir = td.path().join("tasks");
    let store = Store::init(
        td.path(),
        true,
        Some(tasks_dir.to_string_lossy().into_owned()),
    )
    .unwrap();
    (td, store)
}

// --- Subscription dispatch ---------------------------------------------

#[test]
fn run_skips_when_event_not_in_subscriptions() {
    let (_td, store) = stealth_store();
    let p = FakeParticipant::new("fake").with_subs(vec![Event::Review]);
    let r = run(&p, Event::Claim, ctx(&store)).unwrap();
    assert!(matches!(r, NegotiationResult::Skipped(s) if s.contains("does not subscribe")));
}

#[test]
fn run_dispatches_when_event_in_subscriptions() {
    let (_td, store) = stealth_store();
    let p = FakeParticipant::new("fake").with_subs(vec![Event::Claim]);
    let r = run(&p, Event::Claim, ctx(&store)).unwrap();
    let accepted = match r {
        NegotiationResult::Ok(a) => a,
        other => panic!("expected Ok, got {other:?}"),
    };
    assert_eq!(accepted.outcome, "ok");
}

#[test]
fn run_drives_conflict_then_ok_through_fetch_remote_view() {
    // Exercises the FakeProtocol's propose + fetch_remote_view path —
    // the Negotiation loop calls fetch_remote_view between the
    // conflicting propose and the retry. Verifies the participant
    // wiring isn't quietly bypassing the conflict-resolution hooks.
    let (_td, store) = stealth_store();
    let calls = std::cell::RefCell::new(0);
    let p = FakeParticipant::new("fake").with_attempt(Box::new(move || {
        let mut c = calls.borrow_mut();
        *c += 1;
        if *c == 1 { Ok(AttemptClass::Conflict) } else { Ok(AttemptClass::Ok) }
    }));
    let r = run(&p, Event::Claim, ctx(&store)).unwrap();
    let accepted = match r {
        NegotiationResult::Ok(a) => a,
        other => panic!("expected Ok, got {other:?}"),
    };
    assert_eq!(accepted.outcome, "ok");
}

// --- Strict dispatcher ------------------------------------------------

#[test]
fn run_strict_returns_outcome_on_clean_push() {
    let (_td, store) = stealth_store();
    let p = FakeParticipant::new("fake");
    let outcome = run_strict(&p, Event::Claim, ctx(&store)).unwrap();
    assert_eq!(outcome, "ok");
}

#[test]
fn run_strict_collapses_skipped_to_err() {
    let (_td, store) = stealth_store();
    let p = FakeParticipant::new("fake").with_subs(vec![Event::Review]);
    let err = run_strict(&p, Event::Claim, ctx(&store)).unwrap_err();
    assert!(format!("{err}").contains("does not subscribe"));
}

#[test]
fn run_strict_collapses_staged_to_err() {
    let (_td, store) = stealth_store();
    let p = FakeParticipant::new("fake")
        .with_policy(FailurePolicy::Gating)
        .with_attempt(Box::new(|| Ok(AttemptClass::Unreachable("offline".into()))));
    let err = run_strict(&p, Event::Claim, ctx(&store)).unwrap_err();
    assert!(format!("{err}").contains("offline"));
}

// --- Failure-policy branching ------------------------------------------

#[test]
fn required_policy_propagates_unreachable_as_err() {
    let (_td, store) = stealth_store();
    let p = FakeParticipant::new("fake")
        .with_policy(FailurePolicy::Required)
        .with_attempt(Box::new(|| Ok(AttemptClass::Unreachable("offline".into()))));
    let err = run(&p, Event::Claim, ctx(&store)).unwrap_err();
    assert!(format!("{err}").contains("offline"));
}

#[test]
fn best_effort_policy_absorbs_unreachable_as_skipped() {
    let (_td, store) = stealth_store();
    let p = FakeParticipant::new("fake")
        .with_policy(FailurePolicy::BestEffort)
        .with_attempt(Box::new(|| Ok(AttemptClass::Unreachable("offline".into()))));
    let r = run(&p, Event::Claim, ctx(&store)).unwrap();
    assert!(matches!(r, NegotiationResult::Skipped(s) if s.contains("offline")));
}

#[test]
fn gating_policy_absorbs_unreachable_as_staged() {
    let (_td, store) = stealth_store();
    let p = FakeParticipant::new("fake")
        .with_policy(FailurePolicy::Gating)
        .with_attempt(Box::new(|| Ok(AttemptClass::Unreachable("offline".into()))));
    let r = run(&p, Event::Claim, ctx(&store)).unwrap();
    assert!(matches!(r, NegotiationResult::Staged(s) if s.contains("offline")));
}

// --- Metadata accessors ------------------------------------------------

#[test]
fn participant_metadata_round_trips_through_trait() {
    let p = FakeParticipant::new("fake-svc")
        .with_subs(vec![Event::Claim, Event::Review])
        .with_projection(Projection::external_only("fake-svc"));
    assert_eq!(p.name(), "fake-svc");
    assert_eq!(p.subscriptions(), &[Event::Claim, Event::Review]);
    assert!(p.projection().external_prefixes.contains("fake-svc"));
}

/// A participant that lists an event as subscribed but returns
/// `None` from `protocol` — covers the dispatcher's protocol-absent
/// branch. The git-remote impl never does this; FakeParticipant
/// always returns Some, so we synthesize one here.
struct NoProtoParticipant {
    projection: Projection,
}
impl Participant for NoProtoParticipant {
    type Outcome = &'static str;
    type Protocol<'a> = FakeProtocol<'a> where Self: 'a;
    fn name(&self) -> &'static str {
        "no-proto"
    }
    fn subscriptions(&self) -> &[Event] {
        &[Event::Claim]
    }
    fn projection(&self) -> &Projection {
        &self.projection
    }
    fn failure_policy(&self, _event: Event) -> FailurePolicy {
        FailurePolicy::Required
    }
    fn protocol<'a>(&'a self, _e: Event, _c: EventCtx<'a>) -> Option<Self::Protocol<'a>> {
        None
    }
}

#[test]
fn run_returns_skipped_when_protocol_returns_none_for_subscribed_event() {
    let (_td, store) = stealth_store();
    let p = NoProtoParticipant { projection: Projection::default() };
    assert_eq!(*p.projection(), Projection::default());
    let r = run(&p, Event::Claim, ctx(&store)).unwrap();
    assert!(matches!(r, NegotiationResult::Skipped(s) if s.contains("no protocol")));
}
