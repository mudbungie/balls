//! SPEC-tracker-state §16 conformance — `bl remaster`.
//!
//! Tests 3, 9, 10: the address round-trips through `config.json`,
//! `--detach` works offline against an unreachable tracker, and
//! `bl remaster <url>` reconciles local-only tasks onto the target
//! and is idempotent on a second run.

mod common;

use common::tracker::*;
use common::*;

/// Test 3 — Address round-trip. `bl remaster <url>` writes `state_url`
/// to `config.json`; `bl remaster --detach` removes it; with the field
/// absent the address resolves to the implicit `(origin, balls/tasks)`.
#[test]
fn t3_address_round_trip_through_config() {
    let tracker = new_tracker();
    let repo = new_repo();
    init_in(repo.path());
    assert_eq!(state_url(repo.path()), None, "fresh repo carries no address");

    bl(repo.path()).args(["remaster", &url_of(&tracker)]).assert().success();
    assert_eq!(
        state_url(repo.path()).as_deref(),
        Some(url_of(&tracker).as_str()),
        "bl remaster <url> must write state_url into config.json"
    );

    bl(repo.path()).args(["remaster", "--detach"]).assert().success();
    assert_eq!(
        state_url(repo.path()),
        None,
        "bl remaster --detach must remove state_url from config.json"
    );

    // Address absent ⇒ the implicit default still drives a full repo.
    create_task(repo.path(), "post-detach task");
    bl(repo.path()).arg("ready").assert().success();
}

/// Test 9 — Detach offline. `bl remaster --detach` against a tracker
/// it cannot reach still succeeds, reverts the address, and leaves a
/// working standalone store behind.
#[test]
fn t9_detach_offline_against_unreachable_tracker() {
    let repo = new_repo();
    seed_config(repo.path(), &[("state_url", "/no/such/tracker/hub.git")]);
    git(repo.path(), &["add", ".balls/config.json"]);
    git(repo.path(), &["commit", "-qm", "wire state_url", "--no-verify"]);

    bl(repo.path()).args(["remaster", "--detach"]).assert().success();
    assert_eq!(
        state_url(repo.path()),
        None,
        "offline detach must still clear the address"
    );

    // The repo is standalone again — a fresh lifecycle works offline.
    bl(repo.path()).arg("prime").assert().success();
    create_task(repo.path(), "post-detach offline task");
    assert!(repo.path().join(".balls/state-repo/.git").exists());
}

/// Test 10 — Reconcile. `bl remaster <url>` replays the workspace's
/// local-only tasks onto the target history; a second run against the
/// same tracker is a no-op.
#[test]
fn t10_reconcile_replays_local_only_tasks() {
    let tracker = new_tracker();
    let repo = new_repo();
    init_in(repo.path());
    let local = create_task(repo.path(), "local-only task");

    let first = bl(repo.path()).args(["remaster", &url_of(&tracker)]).assert().success();
    let out = String::from_utf8(first.get_output().stdout.clone()).unwrap();
    assert!(out.contains("replayed") || out.contains("joined"), "reconcile summary: {out}");

    // The local-only task survived the join.
    let listed = bl(repo.path()).arg("list").assert().success();
    let listing = String::from_utf8(listed.get_output().stdout.clone()).unwrap();
    assert!(listing.contains(&local), "local-only task must survive the reconcile: {listing}");

    // Second run against the same tracker is idempotent.
    let second = bl(repo.path()).args(["remaster", &url_of(&tracker)]).assert().success();
    let out2 = String::from_utf8(second.get_output().stdout.clone()).unwrap();
    assert!(out2.contains("up to date"), "second remaster must be a no-op: {out2}");
}
