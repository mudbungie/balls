//! Unit coverage for `LegacyPluginParticipant`. Verifies the SPEC §11
//! mapping (`sync_on_change` → subscriptions) and §12 invariants
//! (BestEffort policy, external-only projection, single retry budget).
//! End-to-end push/sync wire behavior is covered by the integration
//! tests under `tests/plugin_*.rs`.

use super::*;
use crate::config::PluginEntry;
use crate::negotiation::{AttemptClass, FailurePolicy, NegotiationResult, Protocol};
use crate::participant::{self, Event, EventCtx, Participant};
use crate::store::Store;
use crate::task::{NewTaskOpts, Task, TaskType};

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

fn entry(sync_on_change: bool) -> PluginEntry {
    PluginEntry {
        enabled: true,
        sync_on_change,
        config_file: ".balls/plugins/x.json".into(),
        participant: None,
    }
}

fn save_task(store: &Store, id: &str) -> Task {
    let opts = NewTaskOpts {
        title: "test".into(),
        task_type: TaskType::task(),
        priority: 3,
        parent: None,
        depends_on: vec![],
        description: String::new(),
        tags: vec![],
    };
    let task = Task::new(opts, id.into());
    store.save_task(&task).unwrap();
    task
}

#[test]
fn sync_on_change_true_subscribes_to_all_events() {
    let (_td, store) = stealth_store();
    let p = LegacyPluginParticipant::from_entry(&store, "x".into(), &entry(true), None);
    let subs = p.subscriptions();
    for ev in [
        Event::Claim,
        Event::Review,
        Event::Close,
        Event::Update,
        Event::Sync,
    ] {
        assert!(subs.contains(&ev), "missing {ev:?}");
    }
}

#[test]
fn sync_on_change_false_subscribes_to_sync_only() {
    let (_td, store) = stealth_store();
    let p =
        LegacyPluginParticipant::from_entry(&store, "x".into(), &entry(false), None);
    assert_eq!(p.subscriptions(), &[Event::Sync]);
}

#[test]
fn projection_is_external_only_for_plugin_name() {
    let (_td, store) = stealth_store();
    let p =
        LegacyPluginParticipant::from_entry(&store, "jira".into(), &entry(true), None);
    assert!(p.projection().external_prefixes.contains("jira"));
    assert!(p.projection().owns.is_empty());
}

#[test]
fn name_is_the_configured_plugin_name() {
    let (_td, store) = stealth_store();
    let p =
        LegacyPluginParticipant::from_entry(&store, "linear".into(), &entry(true), None);
    assert_eq!(p.name(), "linear");
}

#[test]
fn failure_policy_is_best_effort_for_every_event() {
    // SPEC §12: legacy plugins are best-effort. A required-policy
    // legacy plugin would block the lifecycle event on a single
    // subprocess hiccup, which the existing test corpus assumes
    // never happens.
    let (_td, store) = stealth_store();
    let p = LegacyPluginParticipant::from_entry(&store, "x".into(), &entry(true), None);
    for ev in [
        Event::Claim,
        Event::Review,
        Event::Close,
        Event::Update,
        Event::Sync,
    ] {
        assert!(matches!(p.failure_policy(ev), FailurePolicy::BestEffort));
    }
}

#[test]
fn protocol_for_push_event_loads_the_task() {
    let (_td, store) = stealth_store();
    save_task(&store, "bl-7e57");
    let p = LegacyPluginParticipant::from_entry(&store, "x".into(), &entry(true), None);
    for event in [Event::Claim, Event::Review, Event::Close, Event::Update] {
        let ctx = EventCtx {
            event,
            store: &store,
            task_id: "bl-7e57",
            identity: "alice",
        };
        assert!(matches!(
            p.protocol(event, ctx),
            Some(LegacyProtocol::Push { .. })
        ));
    }
}

#[test]
fn protocol_for_push_event_returns_none_when_task_missing() {
    let (_td, store) = stealth_store();
    let p = LegacyPluginParticipant::from_entry(&store, "x".into(), &entry(true), None);
    let ctx = EventCtx {
        event: Event::Review,
        store: &store,
        task_id: "bl-9999",
        identity: "alice",
    };
    assert!(p.protocol(Event::Review, ctx).is_none());
}

#[test]
fn protocol_for_sync_event_returns_sync_variant() {
    let (_td, store) = stealth_store();
    save_task(&store, "bl-0001");
    let p = LegacyPluginParticipant::from_entry(
        &store,
        "x".into(),
        &entry(true),
        Some("filter".into()),
    );
    let ctx = EventCtx {
        event: Event::Sync,
        store: &store,
        task_id: "",
        identity: "alice",
    };
    assert!(matches!(
        p.protocol(Event::Sync, ctx),
        Some(LegacyProtocol::Sync { .. })
    ));
}

#[test]
fn run_through_participant_primitive_absorbs_unreachable_as_skipped() {
    // Regression guard: asserts that legacy plugins are dispatched
    // through the SPEC §5 negotiation primitive — not a parallel
    // direct-spawn path. A missing executable surfaces as
    // AttemptClass::Other inside the protocol; the BestEffort policy
    // declared by `LegacyPluginParticipant::failure_policy` is what
    // collapses that to NegotiationResult::Skipped. If a future
    // change reroutes legacy plugins around the primitive, this
    // observable becomes Err / Ok / something else and this test
    // breaks.
    let (_td, store) = stealth_store();
    save_task(&store, "bl-7e57");
    let p = LegacyPluginParticipant::from_entry(&store, "ghost".into(), &entry(true), None);
    let ctx = EventCtx {
        event: Event::Claim,
        store: &store,
        task_id: "bl-7e57",
        identity: "alice",
    };
    let saved = std::env::var_os("PATH");
    unsafe {
        std::env::remove_var("PATH");
    }
    let result = participant::run(&p, Event::Claim, ctx).unwrap();
    if let Some(p) = saved {
        unsafe {
            std::env::set_var("PATH", p);
        }
    }
    assert!(
        matches!(&result, NegotiationResult::Skipped(s) if s.contains("auth check failed")),
        "expected Skipped from BestEffort policy, got {result:?}",
    );
}

#[test]
fn dispatch_propose_via_outer_protocol_unreachable_executable() {
    // Drives `LegacyProtocol::propose` for both branches by stripping
    // PATH so auth_check fails. Push and Sync both collapse to
    // AttemptClass::Other; the dispatcher's BestEffort policy will
    // absorb that, but here we exercise the protocol directly.
    let (_td, store) = stealth_store();
    save_task(&store, "bl-0001");
    let p = LegacyPluginParticipant::from_entry(&store, "x".into(), &entry(true), None);
    let ctx = EventCtx {
        event: Event::Claim,
        store: &store,
        task_id: "bl-0001",
        identity: "alice",
    };
    let saved = std::env::var_os("PATH");
    unsafe {
        std::env::remove_var("PATH");
    }
    let mut push_proto = p.protocol(Event::Claim, ctx).unwrap();
    let push_class = push_proto.propose().unwrap();
    let push_outcome = push_proto.pushed();
    let push_budget = push_proto.retry_budget();
    push_proto.fetch_remote_view().unwrap();
    let sync_ctx = EventCtx {
        event: Event::Sync,
        store: &store,
        task_id: "",
        identity: "alice",
    };
    let mut sync_proto = p.protocol(Event::Sync, sync_ctx).unwrap();
    let sync_class = sync_proto.propose().unwrap();
    let sync_outcome = sync_proto.pushed();
    let sync_budget = sync_proto.retry_budget();
    sync_proto.fetch_remote_view().unwrap();
    if let Some(p) = saved {
        unsafe {
            std::env::set_var("PATH", p);
        }
    }
    assert!(matches!(push_class, AttemptClass::Other(_)));
    assert!(matches!(sync_class, AttemptClass::Other(_)));
    assert!(matches!(push_outcome, LegacyOutcome::Push(None)));
    assert!(matches!(sync_outcome, LegacyOutcome::Sync(None)));
    assert_eq!(push_budget, 1);
    assert_eq!(sync_budget, 1);
}
