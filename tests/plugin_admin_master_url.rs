//! bl-32e5 — `bl plugin enable/disable/list` in `master_url` mode.
//! Writes land on the state-repo's `balls/tasks` branch and are
//! committed automatically so `bl sync` can publish them to the hub.

mod common;

use common::*;
use serde_json::Value;
use std::fs;

fn alice_against_hub() -> (Repo, Repo) {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();
    (hub, alice)
}

#[test]
fn enable_master_url_writes_and_commits_state_repo() {
    let (_hub, alice) = alice_against_hub();

    bl(alice.path())
        .args(["plugin", "enable", "github"])
        .assert()
        .success();

    // Effective config in master_url mode lives on the state-repo.
    let state_cfg_path = alice.path().join(".balls/state-repo/.balls/config.json");
    let state_cfg: Value =
        serde_json::from_str(&fs::read_to_string(&state_cfg_path).unwrap()).unwrap();
    assert!(state_cfg["plugins"]
        .as_object()
        .unwrap()
        .contains_key("github"));

    // Per-plugin config file is on the state-repo. The project's
    // `.balls/plugins/` is a bl-1098 symlink into it, not a second
    // copy — so the file is single-sourced on the hub.
    assert!(alice
        .path()
        .join(".balls/state-repo/.balls/plugins/github.json")
        .exists());
    assert!(
        alice.path().join(".balls/plugins").is_symlink(),
        "project .balls/plugins must be a symlink to the hub view (bl-1098)"
    );

    // Project's own config.json is untouched — master wins on the hub.
    let project_cfg: Value =
        serde_json::from_str(&fs::read_to_string(alice.path().join(".balls/config.json")).unwrap())
            .unwrap();
    assert!(project_cfg["plugins"]
        .as_object()
        .is_none_or(|m| !m.contains_key("github")));

    // Commit landed on the state-repo (no dirty working tree).
    let state_repo = alice.path().join(".balls/state-repo");
    let porcelain = git(&state_repo, &["status", "--porcelain"]);
    assert!(
        porcelain.trim().is_empty(),
        "state-repo not clean: {porcelain}"
    );

    let log = git(&state_repo, &["log", "--format=%s", "-1"]);
    assert!(log.contains("plugin enable github"), "last commit: {log}");
}

#[test]
fn list_master_url_reports_hub_source() {
    let (_hub, alice) = alice_against_hub();
    bl(alice.path())
        .args(["plugin", "enable", "github"])
        .assert()
        .success();

    let out = bl(alice.path())
        .args(["plugin", "list", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let parsed: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(parsed["source"], Value::String("hub".into()));
}

#[test]
fn disable_master_url_removes_entry_and_commits() {
    let (_hub, alice) = alice_against_hub();
    bl(alice.path())
        .args(["plugin", "enable", "github"])
        .assert()
        .success();
    bl(alice.path())
        .args(["plugin", "disable", "github"])
        .assert()
        .success();

    let state_cfg: Value = serde_json::from_str(
        &fs::read_to_string(alice.path().join(".balls/state-repo/.balls/config.json")).unwrap(),
    )
    .unwrap();
    assert!(state_cfg["plugins"]
        .as_object()
        .is_none_or(|m| !m.contains_key("github")));
    // The per-plugin config file is intentionally retained on disable.
    assert!(alice
        .path()
        .join(".balls/state-repo/.balls/plugins/github.json")
        .exists());

    let state_repo = alice.path().join(".balls/state-repo");
    let log = git(&state_repo, &["log", "--format=%s", "-1"]);
    assert!(log.contains("plugin disable github"), "last commit: {log}");
}
