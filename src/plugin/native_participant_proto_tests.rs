//! Coverage for `NativeProtocol`'s in-process branches: the
//! propose-classify state machine, fetch_remote_view, and the
//! `pushed` outcome shape. These drive `__test_*` helpers exposed by
//! `native_participant.rs` so tests do not need a real subprocess.

use super::test_helpers::{entry, save_task, stealth_store};
use super::{NativeProtocol, __test_classify};
use crate::plugin::native_types::{CommitPolicyWire, ProposeConflict, ProposeOk, ProposeResponse};
use crate::plugin::Plugin;
use crate::negotiation::{AttemptClass, CommitPolicy, Protocol};
use crate::participant::Event;
use crate::task::Task;
use serde_json::json;

fn make_protocol(plugin: &Plugin, task: Task) -> NativeProtocol<'_> {
    NativeProtocol::__test_new(plugin, "jira", Event::Claim, task, 5)
}

#[test]
fn pushed_uses_default_commit_policy_when_plugin_omits_one() {
    let (_td, store) = stealth_store();
    let plugin = Plugin::resolve(&store, "jira", &entry());
    let task = save_task(&store, "bl-dddd");
    let mut proto = make_protocol(&plugin, task);
    proto.__test_record_ok(ProposeOk {
        task: json!({ "title": "from-plugin" }),
        commit_policy: None,
    });
    let outcome = proto.pushed();
    assert_eq!(outcome.task_projection, json!({ "title": "from-plugin" }));
    assert_eq!(outcome.commit_policy, CommitPolicy::default());
}

#[test]
fn pushed_propagates_plugin_supplied_commit_policy() {
    let (_td, store) = stealth_store();
    let plugin = Plugin::resolve(&store, "jira", &entry());
    let task = save_task(&store, "bl-eeee");
    let mut proto = make_protocol(&plugin, task);
    proto.__test_record_ok(ProposeOk {
        task: json!({}),
        commit_policy: Some(CommitPolicyWire::Batch { tag: "audit".into() }),
    });
    let outcome = proto.pushed();
    assert_eq!(
        outcome.commit_policy,
        CommitPolicy::Batch { tag: "audit".into() }
    );
}

#[test]
fn pushed_yields_null_projection_when_no_ok_recorded() {
    let (_td, store) = stealth_store();
    let plugin = Plugin::resolve(&store, "jira", &entry());
    let task = save_task(&store, "bl-ffff");
    let mut proto = make_protocol(&plugin, task);
    let outcome = proto.pushed();
    assert!(outcome.task_projection.is_null());
}

#[test]
fn fetch_remote_view_clears_pending_conflict_without_mutating_task() {
    // SPEC §8: native plugins manage their own remote-state memory;
    // the framework treats `remote_view` as informational and does
    // not fold it into the working task. Folding outside-projection
    // fields would defeat disjoint-projection composability.
    let (_td, store) = stealth_store();
    let plugin = Plugin::resolve(&store, "jira", &entry());
    let task = save_task(&store, "bl-1111");
    let before = task.title.clone();
    let mut proto = make_protocol(&plugin, task);
    proto.__test_record_conflict(ProposeConflict {
        fields: vec!["title".into()],
        remote_view: json!({ "title": "remote-edit" }),
        hint: None,
    });
    proto.fetch_remote_view().unwrap();
    assert_eq!(proto.__test_task_title(), before);
    assert!(!proto.__test_has_pending_conflict());
}

#[test]
fn fetch_remote_view_is_noop_when_no_pending_conflict() {
    let (_td, store) = stealth_store();
    let plugin = Plugin::resolve(&store, "jira", &entry());
    let task = save_task(&store, "bl-2222");
    let mut proto = make_protocol(&plugin, task);
    let before = proto.__test_task_title();
    proto.fetch_remote_view().unwrap();
    assert_eq!(proto.__test_task_title(), before);
}

#[test]
fn classify_routes_each_branch_correctly() {
    let mut accepted: Option<ProposeOk> = None;
    let mut conflict: Option<ProposeConflict> = None;
    let class = __test_classify(
        ProposeResponse {
            ok: Some(ProposeOk { task: json!({}), commit_policy: None }),
            conflict: None,
        },
        &mut accepted,
        &mut conflict,
        "jira",
    );
    assert!(matches!(class, AttemptClass::Ok));
    assert!(accepted.is_some());
    accepted = None;
    let class = __test_classify(
        ProposeResponse {
            ok: None,
            conflict: Some(ProposeConflict {
                fields: vec![],
                remote_view: json!({}),
                hint: None,
            }),
        },
        &mut accepted,
        &mut conflict,
        "jira",
    );
    assert!(matches!(class, AttemptClass::Conflict));
    assert!(conflict.is_some());
    let class = __test_classify(
        ProposeResponse { ok: None, conflict: None },
        &mut accepted,
        &mut conflict,
        "jira",
    );
    assert!(
        matches!(&class, AttemptClass::Other(s) if s.contains("neither ok nor conflict"))
    );
}
