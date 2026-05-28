//! `bl remaster` — XDG-aware redirect-pointer writer (bl-be70).
//!
//! Phase 1B-7 flipped `bl remaster` to write `.balls/tracker.json` on
//! the code repo's own `balls/tasks` branch checkout (SPEC §6.1)
//! rather than `state_url` on the legacy committed `.balls/config.json`.
//! The federated tracker checkout under `trackers/<enc-state-url>/
//! <enc-state-branch>/` is materialized by `Store::discover` on next
//! run; this command only ever writes (or removes) the pointer.

mod common;

use balls::encoding::{canonicalize_origin, percent_encode_component};
use balls::tracker_json::TrackerJson;
use balls::xdg_paths::own_tracker_checkout;
use common::*;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;

/// Resolve the own-checkout `.balls/tracker.json` path for an XDG
/// clone. The own checkout lives at
/// `<state>/trackers/<enc-origin>/balls%2Ftasks/`.
fn tracker_json_path(repo: &XdgRepo) -> PathBuf {
    let url = repo.remote.path().to_string_lossy().into_owned();
    let enc = percent_encode_component(&canonicalize_origin(&url));
    let own = own_tracker_checkout(&test_xdg_bases(), &enc);
    own.join(".balls/tracker.json")
}

#[test]
fn remaster_writes_tracker_json_on_own_balls_tasks() {
    let xdg = new_xdg_repo();
    let tracker_remote = new_bare_remote();
    let tracker_url = tracker_remote.path().to_string_lossy().into_owned();

    let tj = tracker_json_path(&xdg);
    assert!(!tj.exists(), "fresh XDG clone carries no tracker.json");

    bl(xdg.clone.path())
        .args(["remaster", &tracker_url, "--commit"])
        .assert()
        .success();

    let body = fs::read_to_string(&tj).expect("tracker.json written");
    let parsed = TrackerJson::from_json(&body).expect("valid tracker.json");
    assert_eq!(parsed.state_url, tracker_url);
    assert_eq!(parsed.state_branch, None, "no --branch ⇒ default elided");

    // `--commit` recorded the redirect on the orphan branch.
    let own_log = git(
        tj.parent().unwrap().parent().unwrap(),
        &["log", "--format=%s", "balls/tasks"],
    );
    assert!(
        own_log.lines().any(|l| l.contains("remaster")),
        "balls/tasks log must carry the remaster commit: {own_log}"
    );
}

#[test]
fn remaster_with_branch_records_state_branch() {
    let xdg = new_xdg_repo();
    let tracker_remote = new_bare_remote();
    let tracker_url = tracker_remote.path().to_string_lossy().into_owned();

    bl(xdg.clone.path())
        .args(["remaster", &tracker_url, "--branch", "custom/state"])
        .assert()
        .success();

    let tj = tracker_json_path(&xdg);
    let parsed = TrackerJson::from_json(&fs::read_to_string(&tj).unwrap()).unwrap();
    assert_eq!(parsed.state_url, tracker_url);
    assert_eq!(parsed.state_branch.as_deref(), Some("custom/state"));
}

#[test]
fn remaster_without_commit_leaves_change_uncommitted() {
    let xdg = new_xdg_repo();
    let tracker_remote = new_bare_remote();
    let tracker_url = tracker_remote.path().to_string_lossy().into_owned();

    bl(xdg.clone.path())
        .args(["remaster", &tracker_url])
        .assert()
        .success();

    let tj = tracker_json_path(&xdg);
    assert!(tj.exists(), "file is written even without --commit");
    let own = tj.parent().unwrap().parent().unwrap();
    let status = git(own, &["status", "--porcelain"]);
    assert!(
        status.contains("tracker.json"),
        "tracker.json must show as a dirty file when --commit is omitted: {status}"
    );
}

#[test]
fn detach_removes_tracker_json() {
    let xdg = new_xdg_repo();
    let tracker_remote = new_bare_remote();
    let tracker_url = tracker_remote.path().to_string_lossy().into_owned();

    bl(xdg.clone.path())
        .args(["remaster", &tracker_url, "--commit"])
        .assert()
        .success();
    let tj = tracker_json_path(&xdg);
    assert!(tj.exists(), "remaster wrote tracker.json");

    bl(xdg.clone.path())
        .args(["remaster", "--detach", "--commit"])
        .assert()
        .success();
    assert!(!tj.exists(), "detach removed tracker.json");

    let own_log = git(
        tj.parent().unwrap().parent().unwrap(),
        &["log", "--format=%s", "balls/tasks"],
    );
    assert!(
        own_log.lines().any(|l| l.contains("--detach")),
        "detach must record a commit on balls/tasks: {own_log}"
    );
}

#[test]
fn detach_on_solo_clone_is_a_noop() {
    let xdg = new_xdg_repo();
    let tj = tracker_json_path(&xdg);
    assert!(!tj.exists(), "fresh solo clone has no tracker.json");

    bl(xdg.clone.path())
        .args(["remaster", "--detach"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already detached"));
    assert!(!tj.exists());
}

#[test]
fn remaster_rejects_target_plus_detach() {
    let xdg = new_xdg_repo();
    bl(xdg.clone.path())
        .args(["remaster", "https://example.com/x.git", "--detach"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--detach takes no TARGET"));
}
