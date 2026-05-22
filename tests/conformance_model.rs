//! SPEC-tracker-state §16 conformance — the unified model.
//!
//! Tests 1, 2, 13, 14: the default address *is* the model, one
//! checkout for every repo, the §12 old-`bl` caveat, and the §13
//! hand-operable join sequence. Each must fail against the pre-spec
//! `master_url`-mode code and pass once the checkout is unified.

mod common;

use common::tracker::*;
use common::*;
use std::path::Path;

/// Run a claimed task all the way to closed, from a worktree-bearing
/// repo root. Returns nothing — a panic on any step fails the test.
fn lifecycle(repo: &Path, title: &str) {
    let id = create_task(repo, title);
    let claim = bl(repo).args(["claim", &id]).output().expect("claim");
    assert!(claim.status.success(), "claim: {}", String::from_utf8_lossy(&claim.stderr));
    let wt = String::from_utf8(claim.stdout).unwrap().trim().to_string();
    std::fs::write(Path::new(&wt).join("change.txt"), "work\n").unwrap();
    bl(Path::new(&wt)).args(["review", &id, "-m", "deliver"]).assert().success();
    bl(repo).args(["close", &id, "-m", "done"]).assert().success();
}

/// Test 1 — Default is the model. A repo with no address fields
/// resolves `.balls/state-repo` and runs a full lifecycle; the legacy
/// `.balls/worktree` checkout never appears.
#[test]
fn t1_default_is_the_model() {
    let repo = new_repo();
    init_in(repo.path());

    assert!(
        repo.path().join(".balls/state-repo/.git").exists(),
        "a default-address repo must materialize .balls/state-repo"
    );
    assert!(
        !repo.path().join(".balls/worktree").exists(),
        "the legacy .balls/worktree must not exist under the unified model"
    );
    assert_eq!(state_url(repo.path()), None, "no address field is written by bl init");

    lifecycle(repo.path(), "default model task");
}

/// Test 2 — One checkout. `Store::discover` resolves `.balls/state-repo`
/// for both a default-address repo and an explicit-`state_url` repo,
/// with no `.balls/worktree` and no mode flag in either.
#[test]
fn t2_one_checkout_both_addresses() {
    // Default address.
    let plain = new_repo();
    init_in(plain.path());
    assert!(plain.path().join(".balls/state-repo/.git").exists());
    assert!(!plain.path().join(".balls/worktree").exists());

    // Explicit state_url at a dedicated tracker.
    let tracker = new_tracker();
    let fed = new_repo();
    seed_config(fed.path(), &[("state_url", &url_of(&tracker))]);
    bl(fed.path()).arg("init").assert().success();
    assert!(
        fed.path().join(".balls/state-repo/.git").exists(),
        "an explicit-state_url repo resolves the same .balls/state-repo"
    );
    assert!(!fed.path().join(".balls/worktree").exists());
}

/// Test 13 — Old-`bl` caveat (§12). A workspace with `state_url` set
/// routes task state to the tracker; the new binary never falls back
/// to its own git's `.balls/worktree` (which is exactly what a
/// pre-spec binary, ignorant of the field, would do).
#[test]
fn t13_state_url_routes_to_tracker_not_local_worktree() {
    let tracker = new_tracker();
    let ws = new_repo();
    seed_config(ws.path(), &[("state_url", &url_of(&tracker))]);
    bl(ws.path()).arg("init").assert().success();

    let id = create_task(ws.path(), "routed to tracker");
    bl(ws.path()).arg("sync").assert().success();

    assert!(
        tracker_task_ids(tracker.path()).contains(&id),
        "the task must land on the tracker's balls/tasks, not a local worktree"
    );
    assert!(
        !ws.path().join(".balls/worktree").exists(),
        "a state_url workspace must not resolve its own git's .balls/worktree"
    );
}

/// Test 14 — Hand-operability (§13). The by-hand join sequence — a
/// single-branch clone plus `ln -s` symlinks plus a `config.json`
/// edit — produces a layout `Store::discover` accepts.
#[test]
fn t14_hand_operable_join_sequence() {
    let tracker = new_tracker();
    let ws = new_repo();
    // The workspace carries only its committed config.json with the
    // address — no .balls/state-repo, no symlinks yet.
    seed_config(ws.path(), &[("state_url", &url_of(&tracker))]);

    let balls = ws.path().join(".balls");
    // git clone --single-branch --branch balls/tasks <tracker> .balls/state-repo
    git(
        ws.path(),
        &[
            "clone",
            "-q",
            "--single-branch",
            "--branch",
            "balls/tasks",
            &url_of(&tracker),
            ".balls/state-repo",
        ],
    );
    // ln -sf state-repo/.balls/tasks .balls/tasks ; same for plugins.
    std::os::unix::fs::symlink("state-repo/.balls/tasks", balls.join("tasks")).unwrap();
    std::os::unix::fs::symlink("state-repo/.balls/plugins", balls.join("plugins")).unwrap();

    // Store::discover must accept the hand-built layout.
    bl(ws.path()).arg("list").assert().success();
    let id = create_task(ws.path(), "hand-built workspace");
    let listed = bl(ws.path()).arg("list").assert().success();
    let out = String::from_utf8(listed.get_output().stdout.clone()).unwrap();
    assert!(out.contains(&id), "hand-built workspace must be fully usable: {out}");
}
