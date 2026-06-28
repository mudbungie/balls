//! §7 payload tests: assert each phase's payload carries exactly its fields —
//! `pre` is states-only with no id leak, `post` adds commits + parsed metadata
//! and advances the state pair, `rollback` is a phase shape plus `rolling_back`.

use super::*;
use crate::message::Metadata;
use crate::task::Task;
use serde_json::Value;

fn ctx() -> OpContext {
    let before = Task { title: "before".into(), ..Task::default() };
    OpContext {
        actor: "me@example.com".into(),
        binding: Binding {
            remote: Some("origin".into()),
            stealth: false,
            tasks_branch: "balls/tasks".into(),
            store: "/store".into(),
            landing: "/landing".into(),
            invocation_path: "/proj".into(),
        },
        command: Some(Command { op: "close".into(), body_change: Some("new body".into()), message: None }),
        before: Some(before),
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
    assert_eq!(v["command"]["body_change"], "new body");
    // pre current_state is the op-start (before) state.
    assert_eq!(v["current_state"]["title"], "before");
    // None of the post-only keys are present, and the id is never on pre.
    for absent in ["previous_state", "commit", "previous_commit", "metadata", "rolling_back"] {
        assert!(v.get(absent).is_none(), "pre must omit {absent}");
    }
}

#[test]
fn a_post_payload_adds_commits_metadata_and_carries_the_op_start_state() {
    let c = ctx();
    let mut md = Metadata::new();
    md.insert("bl-id".into(), vec!["bl-1234".into()]);
    let facts = SealFacts { commit: "C1", previous_commit: "T0", metadata: Some(&md) };
    let v = json(&c.wire("tracker", "close", "post", Some(facts), None));
    // post carries the op-start (before) state as previous_state; there is NO
    // post current_state — the landed ball is derived from git (§14; bl-667e, §15).
    assert!(v.get("current_state").is_none(), "post carries no after-state on the wire");
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
    let facts = SealFacts { commit: "C1", previous_commit: "T0", metadata: Some(&md) };
    let v = json(&c.wire("tracker", "close", "post", Some(facts), Some("post")));
    assert_eq!(v["rolling_back"], "post");
    assert_eq!(v["commit"], "C1");
}

#[test]
fn a_diffless_post_carries_the_commit_pair_but_omits_metadata() {
    // §13: a sync/prime post payload carries previous_commit/commit — the
    // checkout tip before/after the op — but no §5 metadata (none was sealed).
    let binding = ctx().binding;
    let c = OpContext::diffless("me@example.com".into(), binding);
    let facts = SealFacts { commit: "T1", previous_commit: "T0", metadata: None };
    let v = json(&c.wire("tracker", "sync", "post", Some(facts), None));
    assert_eq!(v["commit"], "T1");
    assert_eq!(v["previous_commit"], "T0");
    // diffless authors no ball, so both states stay absent even on post.
    for absent in ["metadata", "current_state", "previous_state"] {
        assert!(v.get(absent).is_none(), "diffless post must omit {absent}");
    }
}

#[test]
fn a_create_omits_the_absent_before_state() {
    let mut c = ctx();
    c.before = None;
    let v = json(&c.wire("tracker", "create", "pre", None, None));
    assert!(v.get("current_state").is_none(), "create pre has no before-state");
}

#[test]
fn a_diffless_op_omits_command_and_both_states() {
    let binding = ctx().binding;
    let c = OpContext::diffless("me@example.com".into(), binding);
    let v = json(&c.wire("tracker", "sync", "pre", None, None));
    assert_eq!(v["op"], "sync");
    assert_eq!(v["binding"]["tasks_branch"], "balls/tasks");
    // §13: a sync/prime plugin gets meaning from its binding, not a command.
    for absent in ["command", "current_state", "previous_state"] {
        assert!(v.get(absent).is_none(), "diffless wire must omit {absent}");
    }
}

#[test]
fn a_stealth_binding_omits_the_remote() {
    let mut c = ctx();
    c.binding.remote = None;
    let v = json(&c.wire("tracker", "sync", "pre", None, None));
    assert!(v["binding"].get("remote").is_none());
    assert_eq!(v["binding"]["tasks_branch"], "balls/tasks");
}

#[test]
fn an_explicit_stealth_binding_carries_the_flag_a_tracked_one_omits_it() {
    // §12 `bl prime --stealth`: the declared opt-out rides the wire as
    // `stealth: true`; the ordinary false never serializes, so every
    // pre-existing payload shape is byte-identical.
    let c = ctx();
    let v = json(&c.wire("tracker", "prime", "pre", None, None));
    assert!(v["binding"].get("stealth").is_none(), "tracked binding must omit stealth");
    let mut c = ctx();
    c.binding.remote = None;
    c.binding.stealth = true;
    let v = json(&c.wire("tracker", "prime", "pre", None, None));
    assert_eq!(v["binding"]["stealth"], true);
}
