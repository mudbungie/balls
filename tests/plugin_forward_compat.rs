//! SPEC §17.17 conformance (bl-1b07): an older `bl` meeting a native
//! plugin that subscribes to an event it does not know must keep the
//! events it understands and finish the describe handshake — it must
//! NOT hard-error the parse and silently demote the plugin to the
//! legacy shim, losing its whole native protocol.
//!
//! End-to-end through the real `bl` binary so the runner's describe
//! parse, the dispatcher's native/legacy routing, and the projection
//! apply all run. The unit-level seam coverage lives in
//! `src/plugin/native_types_tests.rs`.

mod common;

use common::native_plugin::{
    create_auth, install_native_plugin_describe, path_with, write_plugin_config,
};
use common::*;

#[test]
fn describe_with_unknown_event_still_negotiates_known_events() {
    // `frobnicate` is an event this build does not know. The plugin
    // also subscribes to the events we DO know (including `create`,
    // which `bl create` now dispatches) and owns `external.jira.*`.
    let bin = install_native_plugin_describe(
        "jira",
        r#"{ "subscriptions": ["create", "claim", "review", "close", "update", "frobnicate"],
   "projection": { "external_prefixes": ["jira"] } }"#,
        r#"
        cat - >/dev/null
        printf '{"ok":{"task":{"external":{"jira":{"remote_key":"FC-1"}}}}}\n'
        exit 0
        "#,
    );
    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");

    // `bl create` dispatches a push event the plugin subscribes to.
    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["create", "unknown-event describe"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bl create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The handshake must not have fallen back to the legacy shim.
    assert!(
        !stderr.contains("describe returned invalid JSON"),
        "describe must parse leniently, not hard-error: {stderr}"
    );

    let id = stdout_id(&out);
    let task = read_task_json(repo.path(), &id);
    // The native protocol stayed active despite the unknown event:
    // the plugin's `external.jira.*` slice landed, which only happens
    // on the native `propose` path (the legacy fallback's `push` arm
    // exits non-zero in this fixture).
    assert_eq!(
        task["external"]["jira"]["remote_key"], "FC-1",
        "native plugin's slice must land — known events still negotiated: {task}"
    );
    assert_eq!(task["status"], "open", "canonical fields untouched: {task}");
}

fn stdout_id(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
