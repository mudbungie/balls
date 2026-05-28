//! SPEC-tracker-state §16 conformance — the unified model.
//!
//! Tests 1, 2, 13, 14: the default address *is* the model, one
//! checkout for every repo, the §12 old-`bl` caveat, and the §13
//! hand-operable join sequence.
//!
//! Phase 1B (bl-213e) flipped `cmd_init` to XDG; the legacy
//! unified-checkout topology these tests exercise — `.balls/state-repo`
//! plus symlinks at the clone root — is still reachable via
//! `legacy_clone()` until bl-be70's XDG-aware remaster ships the new
//! federation model.

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

/// Test 1 — Default is the model. A legacy repo with no address fields
/// resolves `.balls/state-repo` and runs a full lifecycle; the legacy
/// `.balls/worktree` checkout never appears.
#[test]
fn t1_default_is_the_model() {
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");

    assert!(
        ws.join(".balls/state-repo/.git").exists(),
        "a default-address repo must materialize .balls/state-repo"
    );
    assert!(
        !ws.join(".balls/worktree").exists(),
        "the legacy .balls/worktree must not exist under the unified model"
    );
    assert_eq!(state_url(&ws), None, "no address field is written by bl init");

    lifecycle(&ws, "default model task");
}

/// Test 2 — One checkout. `Store::discover` resolves `.balls/state-repo`
/// for both a default-address repo and an explicit-`state_url` repo,
/// with no `.balls/worktree` and no mode flag in either.
#[test]
fn t2_one_checkout_both_addresses() {
    let home = tmp();
    // Default address.
    let (_r1, plain, _u1) = legacy_clone(home.path(), "plain");
    assert!(plain.join(".balls/state-repo/.git").exists());
    assert!(!plain.join(".balls/worktree").exists());

    // Explicit state_url at a dedicated tracker.
    let tracker = new_tracker();
    let (_r2, fed, _u2) = legacy_clone(home.path(), "fed");
    bl(&fed).args(["remaster", &url_of(&tracker)]).assert().success();
    assert!(
        fed.join(".balls/state-repo/.git").exists(),
        "an explicit-state_url repo resolves the same .balls/state-repo"
    );
    assert!(!fed.join(".balls/worktree").exists());
}

/// Test 13 — Old-`bl` caveat (§12 / §16.13). A clone with `state_url`
/// set routes task state to the tracker; the new binary never falls
/// back to its own git's `.balls/worktree` — which is exactly what a
/// pre-spec binary, ignorant of the field, would do.
#[test]
fn t13_state_url_routes_to_tracker_not_local_worktree() {
    let tracker = new_tracker();
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");
    bl(&ws).args(["remaster", &url_of(&tracker)]).assert().success();

    let id = create_task(&ws, "routed to tracker");
    bl(&ws).arg("sync").assert().success();

    assert!(
        tracker_task_ids(tracker.path()).contains(&id),
        "the task must land on the tracker's balls/tasks, not a local worktree"
    );
    assert!(
        !ws.join(".balls/worktree").exists(),
        "a state_url clone must not resolve its own git's .balls/worktree"
    );
}
