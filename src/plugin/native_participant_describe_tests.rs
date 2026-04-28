//! Coverage for `NativePluginParticipant::from_describe` and the
//! per-event subscription / failure-policy / retry-budget knobs that
//! describe controls. Wire-level subprocess behavior lives in
//! `tests/plugin_native_protocol.rs`.

use super::test_helpers::{describe_for, entry, save_task, stealth_store};
use super::{NativePluginParticipant, DEFAULT_NATIVE_RETRY_BUDGET};
use crate::plugin::native_types::{DescribeResponse, ProjectionWire};
use crate::negotiation::{FailurePolicy, NegotiationResult, Protocol};
use crate::participant::{self, Event, EventCtx, Field, Participant};
use crate::participant_config::InvocationOverrides;

#[test]
fn from_describe_intersects_declared_and_configured_events() {
    // Plugin declares only Claim+Review in describe. Config (legacy
    // sync_on_change=true) wants every push event. Result: only the
    // intersection (Claim, Review) ends up subscribed.
    let (_td, store) = stealth_store();
    let p = NativePluginParticipant::from_describe(
        &store,
        "jira".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        describe_for(&[Event::Claim, Event::Review]),
    )
    .unwrap();
    let subs: Vec<Event> = p.subscriptions().to_vec();
    assert_eq!(subs, vec![Event::Claim, Event::Review]);
}

#[test]
fn from_describe_propagates_projection_parse_error() {
    let (_td, store) = stealth_store();
    let bad = DescribeResponse {
        subscriptions: vec![Event::Claim],
        projection: ProjectionWire {
            owns: vec!["this-field-does-not-exist".into()],
            ..ProjectionWire::default()
        },
        retry_budget: None,
    };
    let Err(err) = NativePluginParticipant::from_describe(
        &store,
        "jira".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        bad,
    ) else {
        panic!("expected projection-parse error");
    };
    assert!(format!("{err}").contains("this-field-does-not-exist"));
}

#[test]
fn from_describe_uses_default_retry_budget_when_unspecified() {
    let (_td, store) = stealth_store();
    save_task(&store, "bl-0001");
    let p = NativePluginParticipant::from_describe(
        &store,
        "jira".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        describe_for(&[Event::Claim]),
    )
    .unwrap();
    let ctx = EventCtx {
        event: Event::Claim,
        store: &store,
        task_id: "bl-0001",
        identity: "alice",
    };
    let proto = p.protocol(Event::Claim, ctx).unwrap();
    assert_eq!(proto.retry_budget(), DEFAULT_NATIVE_RETRY_BUDGET);
}

#[test]
fn from_describe_honors_explicit_retry_budget_override() {
    let (_td, store) = stealth_store();
    save_task(&store, "bl-0002");
    let mut d = describe_for(&[Event::Claim]);
    d.retry_budget = Some(2);
    let p = NativePluginParticipant::from_describe(
        &store,
        "jira".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        d,
    )
    .unwrap();
    let ctx = EventCtx {
        event: Event::Claim,
        store: &store,
        task_id: "bl-0002",
        identity: "alice",
    };
    let proto = p.protocol(Event::Claim, ctx).unwrap();
    assert_eq!(proto.retry_budget(), 2);
}

#[test]
fn projection_carries_external_prefix_from_describe() {
    let (_td, store) = stealth_store();
    let p = NativePluginParticipant::from_describe(
        &store,
        "jira".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        describe_for(&[Event::Claim]),
    )
    .unwrap();
    assert!(p.projection().external_prefixes.contains("jira"));
    assert_eq!(p.name(), "jira");
}

#[test]
fn failure_policy_falls_back_to_best_effort_for_unsubscribed_event() {
    // describe declares only Claim, but the caller asks for Close;
    // there's no resolved policy entry for Close, so the participant
    // returns the SPEC §12 default.
    let (_td, store) = stealth_store();
    let p = NativePluginParticipant::from_describe(
        &store,
        "jira".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        describe_for(&[Event::Claim]),
    )
    .unwrap();
    assert!(matches!(
        p.failure_policy(Event::Close),
        FailurePolicy::BestEffort
    ));
}

#[test]
fn protocol_returns_none_when_task_missing() {
    let (_td, store) = stealth_store();
    let p = NativePluginParticipant::from_describe(
        &store,
        "jira".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        describe_for(&[Event::Claim]),
    )
    .unwrap();
    let ctx = EventCtx {
        event: Event::Claim,
        store: &store,
        task_id: "bl-9999",
        identity: "alice",
    };
    assert!(p.protocol(Event::Claim, ctx).is_none());
}

#[test]
fn unsubscribed_event_is_skipped_at_run_level() {
    // Plugin subscribed only to Claim; running at Review must surface
    // as Skipped without going to the wire.
    let (_td, store) = stealth_store();
    save_task(&store, "bl-0003");
    let p = NativePluginParticipant::from_describe(
        &store,
        "jira".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        describe_for(&[Event::Claim]),
    )
    .unwrap();
    let ctx = EventCtx {
        event: Event::Review,
        store: &store,
        task_id: "bl-0003",
        identity: "alice",
    };
    let r = participant::run(&p, Event::Review, ctx).unwrap();
    assert!(matches!(r, NegotiationResult::Skipped(_)));
}

#[test]
fn run_against_missing_executable_collapses_to_skipped() {
    // No plugin executable on PATH; the protocol's auth_check fails,
    // propose returns AttemptClass::Other, BestEffort policy absorbs
    // it as Skipped.
    let (_td, store) = stealth_store();
    save_task(&store, "bl-0004");
    let p = NativePluginParticipant::from_describe(
        &store,
        "ghost".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        describe_for(&[Event::Claim]),
    )
    .unwrap();
    let ctx = EventCtx {
        event: Event::Claim,
        store: &store,
        task_id: "bl-0004",
        identity: "alice",
    };
    let saved = std::env::var_os("PATH");
    unsafe {
        std::env::remove_var("PATH");
    }
    let r = participant::run(&p, Event::Claim, ctx).unwrap();
    if let Some(s) = saved {
        unsafe {
            std::env::set_var("PATH", s);
        }
    }
    assert!(
        matches!(&r, NegotiationResult::Skipped(s) if s.contains("auth-check failed")),
        "expected Skipped from BestEffort, got {r:?}"
    );
}

#[test]
fn projection_can_declare_canonical_owns() {
    let (_td, store) = stealth_store();
    let mut d = describe_for(&[Event::Close]);
    d.projection.owns = vec!["status".into(), "external".into()];
    let p = NativePluginParticipant::from_describe(
        &store,
        "jira".into(),
        &entry(),
        None,
        &InvocationOverrides::default(),
        d,
    )
    .unwrap();
    assert!(p.projection().owns.contains(&Field::Status));
    assert!(p.projection().owns.contains(&Field::External));
}
