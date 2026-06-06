//! §7 payload tests: assert each phase's payload carries exactly its fields —
//! `pre` is states-only with no id leak, `post` adds commits + parsed metadata
//! and advances the state pair, `rollback` is a phase shape plus `rolling_back`.

use super::*;
use crate::message::Metadata;
use crate::task::Task;
use serde_json::Value;

fn ctx() -> OpContext {
    let before = Task { title: "before".into(), ..Task::default() };
    let after = Task { title: "after".into(), ..Task::default() };
    OpContext {
        actor: "me@example.com".into(),
        binding: Binding {
            remote: Some("origin".into()),
            branch: "balls".into(),
            operating: "/op".into(),
            invocation_path: "/proj".into(),
        },
        command: Command {
            op: "close".into(),
            field_changes: vec![FieldChange { field: "claimant".into(), value: None }],
            body_change: Some("new body".into()),
        },
        before: Some(before),
        after: Some(after),
    }
}

fn json(p: &Payload) -> Value {
    serde_json::from_str(&serde_json::to_string(p).unwrap()).unwrap()
}

#[test]
fn a_pre_payload_is_states_only_with_no_commit_or_id() {
    let c = ctx();
    let v = json(&c.wire("tracker", "close", "pre", None, None));
    assert_eq!(v["protocol"], 1);
    assert_eq!(v["op"], "close");
    assert_eq!(v["phase"], "pre");
    assert_eq!(v["plugin_name"], "tracker");
    assert_eq!(v["actor"], "me@example.com");
    assert_eq!(v["binding"]["remote"], "origin");
    assert_eq!(v["command"]["op"], "close");
    // pre current_state is the op-start (before) state.
    assert_eq!(v["current_state"]["title"], "before");
    // None of the post-only keys are present, and the id is never on pre.
    for absent in ["previous_state", "commit", "previous_commit", "metadata", "rolling_back"] {
        assert!(v.get(absent).is_none(), "pre must omit {absent}");
    }
}

#[test]
fn a_post_payload_adds_commits_metadata_and_advances_the_state_pair() {
    let c = ctx();
    let mut md = Metadata::new();
    md.insert("bl-id".into(), vec!["bl-1234".into()]);
    let facts = SealFacts { commit: "C1", previous_commit: "T0", metadata: &md };
    let v = json(&c.wire("tracker", "close", "post", Some(facts), None));
    // post advances: current is the after-state, before slides to previous.
    assert_eq!(v["current_state"]["title"], "after");
    assert_eq!(v["previous_state"]["title"], "before");
    assert_eq!(v["commit"], "C1");
    assert_eq!(v["previous_commit"], "T0");
    assert_eq!(v["metadata"]["bl-id"][0], "bl-1234");
    assert!(v.get("rolling_back").is_none());
}

#[test]
fn a_rollback_payload_carries_the_phase_shape_plus_rolling_back() {
    let c = ctx();
    let md = Metadata::new();
    let facts = SealFacts { commit: "C1", previous_commit: "T0", metadata: &md };
    let v = json(&c.wire("tracker", "close", "post", Some(facts), Some("post")));
    assert_eq!(v["rolling_back"], "post");
    assert_eq!(v["commit"], "C1");
}

#[test]
fn a_create_omits_the_absent_before_state() {
    let mut c = ctx();
    c.before = None;
    let v = json(&c.wire("tracker", "create", "pre", None, None));
    assert!(v.get("current_state").is_none(), "create pre has no before-state");
}

#[test]
fn a_clearing_field_change_serializes_a_null_value() {
    let c = ctx();
    let v = json(&c.wire("tracker", "close", "pre", None, None));
    assert_eq!(v["command"]["field_changes"][0]["field"], "claimant");
    assert!(v["command"]["field_changes"][0]["value"].is_null());
    assert_eq!(v["command"]["body_change"], "new body");
}

#[test]
fn a_stealth_binding_omits_the_remote() {
    let mut c = ctx();
    c.binding.remote = None;
    let v = json(&c.wire("tracker", "sync", "pre", None, None));
    assert!(v["binding"].get("remote").is_none());
    assert_eq!(v["binding"]["branch"], "balls");
}
