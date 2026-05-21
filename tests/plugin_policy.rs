//! bl-5cc2 — `bl plugin policy` / `bl plugin show` end-to-end.
//!
//! Drives the real `bl` binary against a standalone repo: policy
//! edits land in the project's `.balls/config.json` in place.

mod common;

use common::*;
use serde_json::Value;
use std::fs;

fn ready_repo() -> Repo {
    let repo = new_repo();
    init_in(repo.path());
    repo
}

fn config(repo: &Repo) -> Value {
    let raw = fs::read_to_string(repo.path().join(".balls/config.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn enable(repo: &Repo, name: &str) {
    bl(repo.path())
        .args(["plugin", "enable", name])
        .assert()
        .success();
}

#[test]
fn policy_set_writes_participant_block() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    bl(repo.path())
        .args(["plugin", "policy", "watcher", "create=required", "update=gating"])
        .assert()
        .success();
    let cfg = config(&repo);
    let subs = &cfg["plugins"]["watcher"]["participant"]["subscriptions"];
    assert_eq!(subs["create"]["policy"], "required");
    assert_eq!(subs["update"]["policy"], "gating");
}

#[test]
fn policy_rm_drops_event_but_keeps_block() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    bl(repo.path())
        .args(["plugin", "policy", "watcher", "create=required"])
        .assert()
        .success();
    bl(repo.path())
        .args(["plugin", "policy", "watcher", "--rm", "create"])
        .assert()
        .success();
    // Dropping the last event leaves an explicit empty `{}`, not None.
    let cfg = config(&repo);
    let part = &cfg["plugins"]["watcher"]["participant"];
    assert!(part.is_object(), "participant block must survive --rm");
    assert_eq!(part["subscriptions"].as_object().unwrap().len(), 0);
}

#[test]
fn policy_clear_removes_block_entirely() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    bl(repo.path())
        .args(["plugin", "policy", "watcher", "create=required"])
        .assert()
        .success();
    bl(repo.path())
        .args(["plugin", "policy", "watcher", "--clear"])
        .assert()
        .success();
    let cfg = config(&repo);
    assert!(
        cfg["plugins"]["watcher"].get("participant").is_none(),
        "--clear must drop the participant key (legacy fallback)"
    );
}

#[test]
fn policy_no_legacy_writes_explicit_empty_map() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    let out = bl(repo.path())
        .args(["plugin", "policy", "watcher", "--no-legacy"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("explicit empty"));
    let cfg = config(&repo);
    let part = &cfg["plugins"]["watcher"]["participant"];
    assert!(part.is_object());
    assert_eq!(part["subscriptions"].as_object().unwrap().len(), 0);
}

#[test]
fn policy_rejects_unknown_plugin() {
    let repo = ready_repo();
    let out = bl(repo.path())
        .args(["plugin", "policy", "ghost", "create=required"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no plugin named"));
}

#[test]
fn policy_rejects_drop_with_non_best_effort() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    let out = bl(repo.path())
        .args(["plugin", "policy", "watcher", "drop=required"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("observe-only"));
}

#[test]
fn policy_rejects_unknown_event_and_kind() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    let bad_event = bl(repo.path())
        .args(["plugin", "policy", "watcher", "bogus=required"])
        .output()
        .unwrap();
    assert!(!bad_event.status.success());
    assert!(String::from_utf8_lossy(&bad_event.stderr).contains("unknown event"));
    let bad_kind = bl(repo.path())
        .args(["plugin", "policy", "watcher", "create=loud"])
        .output()
        .unwrap();
    assert!(!bad_kind.status.success());
    assert!(String::from_utf8_lossy(&bad_kind.stderr).contains("unknown policy"));
}

#[test]
fn policy_rejects_no_operation() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    let out = bl(repo.path())
        .args(["plugin", "policy", "watcher"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nothing to do"));
}

#[test]
fn policy_rejects_conflicting_forms() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    bl(repo.path())
        .args(["plugin", "policy", "watcher", "--clear", "--no-legacy"])
        .assert()
        .failure();
}

#[test]
fn policy_rm_on_legacy_block_errors() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    let out = bl(repo.path())
        .args(["plugin", "policy", "watcher", "--rm", "create"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no participant block"));
}

#[test]
fn show_text_renders_explicit_policy() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    bl(repo.path())
        .args(["plugin", "policy", "watcher", "update=gating"])
        .assert()
        .success();
    let out = bl(repo.path())
        .args(["plugin", "show", "watcher"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("explicit (1-events)"), "stdout: {stdout}");
    assert!(stdout.contains("update"), "stdout: {stdout}");
    assert!(stdout.contains("gating"), "stdout: {stdout}");
}

#[test]
fn show_text_renders_legacy_fallback() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    let out = bl(repo.path())
        .args(["plugin", "show", "watcher"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("legacy"));
}

#[test]
fn show_text_renders_silent_plugin() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    bl(repo.path())
        .args(["plugin", "policy", "watcher", "--no-legacy"])
        .assert()
        .success();
    let out = bl(repo.path())
        .args(["plugin", "show", "watcher"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("plugin is silent"), "stdout: {stdout}");
    assert!(stdout.contains("(none)"), "stdout: {stdout}");
}

#[test]
fn show_json_carries_entry_and_resolved() {
    let repo = ready_repo();
    enable(&repo, "watcher");
    bl(repo.path())
        .args(["plugin", "policy", "watcher", "sync=required"])
        .assert()
        .success();
    let out = bl(repo.path())
        .args(["plugin", "show", "watcher", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["explicit"], Value::Bool(true));
    assert_eq!(
        parsed["resolved"]["subscriptions"]["sync"]["policy"],
        "required"
    );
    assert_eq!(parsed["entry"]["enabled"], Value::Bool(true));
}

#[test]
fn show_rejects_unknown_plugin() {
    let repo = ready_repo();
    let out = bl(repo.path())
        .args(["plugin", "show", "ghost"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no plugin named"));
}

#[test]
fn enable_sync_on_change_warns_deprecated() {
    let repo = ready_repo();
    let out = bl(repo.path())
        .args(["plugin", "enable", "watcher", "--sync-on-change"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("deprecated"), "stderr: {stderr}");
    assert!(stderr.contains("bl plugin policy"), "stderr: {stderr}");
}
