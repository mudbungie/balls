//! bl-32e5 — `bl plugin enable/disable/list` end-to-end.
//!
//! In every clone (standalone or shared-tracker, post-bl-8a9a),
//! plugin admin lands the writes on the state checkout's `balls/tasks`
//! and commits them, so a `bl sync` will push the change to the tracker.

mod common;

use common::*;
use serde_json::Value;
use std::fs;

#[test]
fn enable_standalone_inserts_entry_and_creates_file() {
    let repo = new_repo();
    init_in(repo.path());

    bl(repo.path())
        .args(["plugin", "enable", "github", "--sync-on-change"])
        .assert()
        .success();

    let cfg: Value =
        serde_json::from_str(&fs::read_to_string(project_config_path(repo.path())).unwrap())
            .unwrap();
    let entry = &cfg["plugins"]["github"];
    assert_eq!(entry["enabled"], Value::Bool(true));
    assert_eq!(entry["sync_on_change"], Value::Bool(true));
    // bl-1d81: config_file is clone-root-relative — the same base
    // `Plugin::resolve` joins against at runtime.
    assert_eq!(
        entry["config_file"],
        Value::String(".balls/plugins/github.json".into())
    );
    assert!(plugin_config_root(repo.path()).join(".balls/plugins/github.json").exists());
}

#[test]
fn enable_standalone_respects_explicit_config_file() {
    let repo = new_repo();
    init_in(repo.path());

    bl(repo.path())
        .args(["plugin", "enable", "ci", "--config-file", ".balls/plugins/ci/conf.json"])
        .assert()
        .success();

    let cfg: Value =
        serde_json::from_str(&fs::read_to_string(project_config_path(repo.path())).unwrap())
            .unwrap();
    assert_eq!(
        cfg["plugins"]["ci"]["config_file"],
        Value::String(".balls/plugins/ci/conf.json".into())
    );
    assert!(plugin_config_root(repo.path()).join(".balls/plugins/ci/conf.json").exists());
}

#[test]
fn disable_removes_entry_keeps_config_file() {
    let repo = new_repo();
    init_in(repo.path());

    bl(repo.path())
        .args(["plugin", "enable", "github"])
        .assert()
        .success();
    let file = plugin_config_root(repo.path()).join(".balls/plugins/github.json");
    assert!(file.exists());

    bl(repo.path())
        .args(["plugin", "disable", "github"])
        .assert()
        .success();
    let cfg: Value =
        serde_json::from_str(&fs::read_to_string(project_config_path(repo.path())).unwrap())
            .unwrap();
    assert!(cfg["plugins"]
        .as_object()
        .is_none_or(|m| !m.contains_key("github")));
    assert!(file.exists(), "per-plugin config file must survive disable");
}

#[test]
fn disable_rejects_unknown_plugin() {
    let repo = new_repo();
    init_in(repo.path());

    let out = bl(repo.path())
        .args(["plugin", "disable", "never-enabled"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no plugin"), "stderr: {stderr}");
}

#[test]
fn list_json_shows_enabled_entries() {
    let repo = new_repo();
    init_in(repo.path());

    bl(repo.path())
        .args(["plugin", "enable", "github"])
        .assert()
        .success();

    let out = bl(repo.path())
        .args(["plugin", "list", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(parsed["plugins"]
        .as_object()
        .unwrap()
        .contains_key("github"));
}

#[test]
fn list_empty_text_output_reports_no_plugins() {
    let repo = new_repo();
    init_in(repo.path());

    let out = bl(repo.path()).args(["plugin", "list"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("no plugins"), "stdout: {stdout}");
}

#[test]
fn list_text_renders_enabled_entry() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["plugin", "enable", "github", "--sync-on-change"])
        .assert()
        .success();

    let out = bl(repo.path()).args(["plugin", "list"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("github"), "stdout: {stdout}");
    assert!(stdout.contains("+sync"), "stdout: {stdout}");
    assert!(stdout.contains("github.json"), "stdout: {stdout}");
}

#[test]
fn re_enable_reports_using_existing_file() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["plugin", "enable", "github"])
        .assert()
        .success();
    // Re-enable: the per-plugin file is already there, so the
    // command should report "using existing" instead of "created".
    let out = bl(repo.path())
        .args(["plugin", "enable", "github"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("using existing"), "stdout: {stdout}");
}

#[test]
fn list_renders_participant_subscription_count() {
    let repo = new_repo();
    init_in(repo.path());
    // Seed a plugins entry with a participant block by hand —
    // bl-32e5 deliberately doesn't expose participant editing.
    let cfg_path = project_config_path(repo.path());
    let mut cfg: Value = serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["plugins"] = serde_json::json!({
        "watcher": {
            "enabled": true,
            "sync_on_change": false,
            "config_file": "watcher.json",
            "participant": {
                "subscriptions": {
                    "create": { "policy": "best-effort" },
                    "update": { "policy": "best-effort" }
                }
            }
        }
    });
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    let out = bl(repo.path()).args(["plugin", "list"]).output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("participant=2-events"), "stdout: {stdout}");
}

#[test]
fn enable_rejects_invalid_name() {
    let repo = new_repo();
    init_in(repo.path());

    let out = bl(repo.path())
        .args(["plugin", "enable", "../escape"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ASCII") || stderr.contains("invalid"),
        "stderr: {stderr}"
    );
}
