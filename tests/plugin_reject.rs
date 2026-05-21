//! SPEC §8.1 / §17.19 conformance (bl-2062), end-to-end slice.
//!
//! `reject` is a first-class veto, distinct from `ok`. The full
//! failure-policy mapping (required -> abort with the reason verbatim;
//! best-effort -> warn and continue; gating -> stage; no retry) is the
//! negotiation primitive's contract and is conformance-tested there
//! (`src/negotiation_reject_tests.rs`) — that is the layer bl-2062
//! delivers. The command-level *consumption* of a required `Err`
//! (rollback / non-zero exit) is bl-2bf7 per SPEC §14: today every
//! lifecycle command discards the push-dispatch result by design, so
//! asserting a non-zero exit here would be testing unbuilt bl-2bf7.
//!
//! What this file pins end-to-end through the real binary is the part
//! that IS wired: the dispatcher routes a native `reject` distinctly
//! from a native `ok`. An `ok` plugin contributes its slice; a
//! `reject` plugin contributes nothing and the event still proceeds.
//! Same harness, same event, only the propose branch differs — so the
//! contrast proves `reject` is recognized, not silently applied or
//! crashed.

mod common;

use common::native_plugin::{create_auth, install_native_plugin, path_with};
use common::*;
use std::path::Path;

fn write_jira_config(repo: &Path) {
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
                "participant": { "subscriptions": { "update": { "policy": "best-effort" } } }
            }
        }),
    );
    commit_state_repo(repo, "configure jira");
}

fn run_update(repo: &Repo, bin: &Path, id: &str) -> std::process::Output {
    bl(repo.path())
        .env("PATH", path_with(&[bin]))
        .args(["update", id, "--note", "poke"])
        .output()
        .unwrap()
}

#[test]
fn ok_plugin_contributes_its_slice() {
    // Control: the same harness with a plugin that accepts. Its
    // `external.jira.*` slice must land — proving the dispatch path is
    // live and the absence in the reject case below is the veto, not
    // a dead plugin.
    let bin = install_native_plugin(
        "jira",
        r#"
        cat - >/dev/null
        printf '{"ok":{"task":{"external":{"jira":{"remote_key":"OK-1"}}}}}\n'
        exit 0
        "#,
    );
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "reject-control-ok");
    write_jira_config(repo.path());
    create_auth(repo.path(), "jira");

    let out = run_update(&repo, bin.path(), &id);
    assert!(
        out.status.success(),
        "ok update failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task = read_task_json(repo.path(), &id);
    assert_eq!(
        task["external"]["jira"]["remote_key"], "OK-1",
        "an accepting plugin contributes its slice: {task}"
    );
}

#[test]
fn reject_plugin_ships_and_contributes_nothing() {
    // Same harness, same event — only the propose branch differs.
    // The veto is recognized: no slice lands, and (best-effort) the
    // event still proceeds.
    let bin = install_native_plugin(
        "jira",
        r#"
        cat - >/dev/null
        printf '{"reject":{"reason":"ci is red on this branch"}}\n'
        exit 0
        "#,
    );
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "reject-veto");
    write_jira_config(repo.path());
    create_auth(repo.path(), "jira");

    let out = run_update(&repo, bin.path(), &id);
    assert!(
        out.status.success(),
        "a best-effort reject must not abort: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task = read_task_json(repo.path(), &id);
    assert!(
        task.get("external").and_then(|e| e.get("jira")).is_none(),
        "a vetoed plugin contributes no state: {task}"
    );
}
