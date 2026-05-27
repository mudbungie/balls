//! SPEC §6.1 / §6.2 / §17.20 conformance (bl-ec62), end-to-end.
//!
//! - `create` is a first-class, describe-gated event: a native
//!   plugin that declares it fires on `bl create`; one that declares
//!   only `update` does NOT fire on create (proving create is gated
//!   distinctly from update — same plugin, only the event differs).
//! - `drop` is observe-only: a native plugin that declares it is
//!   notified on `bl drop`, best-effort. The notification cannot
//!   block the release — the plugin vetoes and the drop still
//!   succeeds. A `required`/`gating` policy on `drop` is a config
//!   validation error (§6.2).

mod common;

use common::native_plugin::{
    create_auth, install_native_plugin_describe, path_with, write_plugin_config,
};
use common::*;
use std::path::Path;

const OK_SLICE: &str = r#"
    cat - >/dev/null
    printf '{"ok":{"task":{"external":{"jira":{"seen":"yes"}}}}}\n'
    exit 0
"#;

fn jira_slice(task: &serde_json::Value) -> Option<&serde_json::Value> {
    task.get("external").and_then(|e| e.get("jira"))
}

#[test]
fn create_fires_a_plugin_that_declares_it() {
    let bin = install_native_plugin_describe(
        "jira",
        r#"{ "subscriptions": ["create"],
   "projection": { "external_prefixes": ["jira"] } }"#,
        OK_SLICE,
    );
    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["create", "birth"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let task = read_task_json(repo.path(), &id);
    assert_eq!(
        jira_slice(&task).and_then(|j| j.get("seen")).and_then(|v| v.as_str()),
        Some("yes"),
        "a plugin declaring `create` must fire on bl create: {task}"
    );
}

#[test]
fn create_does_not_fire_a_plugin_that_declares_only_update() {
    // Same plugin, same harness — only the event differs. It owns
    // `external.jira.*` and declares ONLY `update`. `bl create` must
    // not invoke it; a later `bl update` must. That contrast is the
    // proof that `create` is gated distinctly, not that the plugin is
    // dead.
    let bin = install_native_plugin_describe(
        "jira",
        r#"{ "subscriptions": ["update"],
   "projection": { "external_prefixes": ["jira"] } }"#,
        OK_SLICE,
    );
    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");

    let create = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["create", "no-create-sub"])
        .output()
        .unwrap();
    assert!(create.status.success());
    let id = String::from_utf8_lossy(&create.stdout).trim().to_string();
    assert!(
        jira_slice(&read_task_json(repo.path(), &id)).is_none(),
        "create must NOT invoke a plugin that declares only update"
    );

    let update = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--note", "poke"])
        .output()
        .unwrap();
    assert!(update.status.success());
    assert_eq!(
        jira_slice(&read_task_json(repo.path(), &id))
            .and_then(|j| j.get("seen"))
            .and_then(|v| v.as_str()),
        Some("yes"),
        "the same plugin DOES fire on update — so create-gating is real"
    );
}

fn write_drop_config(repo: &Path, policy: &str) {
    let plugins_dir = repo.join(".balls/plugins");
    std::fs::create_dir_all(&plugins_dir).unwrap();
    std::fs::write(plugins_dir.join("jira.json"), "{}").unwrap();
    set_project_plugins(
        repo,
        serde_json::json!({
            "jira": {
                "enabled": true,
                "sync_on_change": false,
                "config_file": ".balls/plugins/jira.json",
                "participant": { "subscriptions": { "drop": { "policy": policy } } }
            }
        }),
    );
    commit_state_repo(repo, "configure jira drop");
}

#[test]
fn drop_notifies_observer_best_effort_and_cannot_block() {
    // The plugin declares `drop`, and on the drop event writes a
    // marker into its auth dir then VETOES. Observe-only: the marker
    // proves it was notified; the veto must not stop the release.
    let bin = install_native_plugin_describe(
        "jira",
        r#"{ "subscriptions": ["drop"],
   "projection": { "external_prefixes": ["jira"] } }"#,
        r#"
        cat - >/dev/null
        if [ "$EVENT" = drop ]; then : > "$STATE_DIR/dropped"; fi
        printf '{"reject":{"reason":"observed walk-away"}}\n'
        exit 0
        "#,
    );
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "to-drop");
    write_drop_config(repo.path(), "best-effort");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["drop", &id])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "an observer must never block a drop: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let marker = plugins_auth_dir(repo.path()).join("jira/dropped");
    assert!(marker.exists(), "the drop observer must have been notified");
    assert_eq!(
        read_task_json(repo.path(), &id)["status"],
        "open",
        "drop still released the claim"
    );
}

#[test]
fn drop_with_required_policy_is_a_config_validation_error() {
    let bin = install_native_plugin_describe("jira", r#"{ "subscriptions": ["drop"],
   "projection": { "external_prefixes": ["jira"] } }"#, OK_SLICE);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "bad-drop-policy");
    write_drop_config(repo.path(), "required");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["drop", &id])
        .output()
        .unwrap();
    assert!(!out.status.success(), "required-on-drop must be rejected");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("observe-only") || stderr.contains("drop"),
        "error must explain drop is observe-only: {stderr}"
    );
}
