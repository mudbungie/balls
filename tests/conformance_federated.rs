//! SPEC-tracker-state §16 conformance — the federated topology.
//!
//! Test 11: federation routes task *state* to the tracker, never
//! code — `bl review`'s squash and `[bl-xxxx]` tag land on the
//! clone's own `origin`. Test 12: the merge-cleanliness gate,
//! run unconditionally in the two-participant topology.

mod common;

use common::tracker::*;
use common::*;
use std::path::Path;

/// Claim, deliver, and close one task from `ws` (origin = code remote).
fn deliver(ws: &Path, title: &str) -> String {
    let id = create_task(ws, title);
    let claim = bl(ws).args(["claim", &id]).output().expect("claim");
    assert!(claim.status.success(), "{}", String::from_utf8_lossy(&claim.stderr));
    let wt = String::from_utf8(claim.stdout).unwrap().trim().to_string();
    std::fs::write(Path::new(&wt).join("feature.rs"), "code\n").unwrap();
    bl(Path::new(&wt)).args(["review", &id, "-m", "deliver feature"]).assert().success();
    bl(ws).args(["close", &id, "-m", "shipped"]).assert().success();
    id
}

/// Test 11 — Code/state split. In a federated clone the squash and
/// the `[bl-xxxx]` delivery tag land on the clone's own `origin`;
/// only the state-branch transition reaches the tracker.
#[test]
fn t11_code_and_state_route_to_different_remotes() {
    let code = new_bare_remote();
    let tracker = new_tracker();

    let ws = clone_from_remote(code.path(), "ws");
    init_in(ws.path());
    bl(ws.path()).args(["remaster", &url_of(&tracker), "--commit"]).assert().success();
    git(ws.path(), &["push", "origin", "main"]);

    let id = deliver(ws.path(), "feature work");
    bl(ws.path()).arg("sync").assert().success();

    // The squash + delivery tag landed on the code remote's `main`.
    let code_log = git(code.path(), &["log", "--format=%s", "main"]);
    assert!(
        code_log.contains(&format!("[{id}]")),
        "the delivery tag must land on the clone's own origin: {code_log}"
    );
    // The tracker carries the task-state lifecycle but never the code squash.
    let tracker_log = git(tracker.path(), &["log", "--format=%s", "balls/tasks"]);
    assert!(
        tracker_log.contains(&id),
        "the task-state lifecycle must reach the tracker: {tracker_log}"
    );
    assert!(
        !tracker_log.contains("deliver feature"),
        "the code squash must not reach the tracker: {tracker_log}"
    );
}

/// Test 12 — Merge cleanliness, two-participant federated topology.
/// Disjoint-field edits to one task merge clean; a same-field edit is
/// resolved deterministically; concurrent note appends union-merge.
/// The non-negotiable gate — run unconditionally.
#[test]
fn t12_merge_cleanliness_two_participants() {
    let tracker = new_tracker();
    let url = url_of(&tracker);

    let alice = new_repo();
    seed_config(alice.path(), &[("state_url", &url)]);
    bl(alice.path()).arg("init").assert().success();
    let bob = new_repo();
    seed_config(bob.path(), &[("state_url", &url)]);
    bl(bob.path()).arg("init").assert().success();

    // Alice files a task; Bob picks it up off the tracker.
    let t = create_task(alice.path(), "shared task");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    // Disjoint fields of the same task — must merge with no conflict.
    bl(alice.path()).args(["update", &t, "priority=1"]).assert().success();
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).args(["update", &t, "description=bob-was-here"]).assert().success();
    bl(bob.path()).arg("sync").assert().success();
    bl(alice.path()).arg("sync").assert().success();
    let merged = read_task_json(alice.path(), &t);
    assert_eq!(merged["priority"], 1, "Alice's disjoint edit survives");
    assert_eq!(merged["description"], "bob-was-here", "Bob's disjoint edit survives");

    // Concurrent note appends union-merge — both notes survive.
    bl(alice.path()).args(["update", &t, "--note", "note-from-alice"]).assert().success();
    bl(bob.path()).args(["update", &t, "--note", "note-from-bob"]).assert().success();
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();
    bl(alice.path()).arg("sync").assert().success();
    let notes = read_task_notes(alice.path(), &t);
    let notes_json = serde_json::to_string(&notes).unwrap();
    assert!(notes_json.contains("note-from-alice"), "Alice's note survives: {notes_json}");
    assert!(notes_json.contains("note-from-bob"), "Bob's note survives: {notes_json}");

    // Same-field edit is a genuine conflict — the resolver settles it
    // deterministically rather than aborting the sync.
    bl(alice.path()).args(["update", &t, "priority=2"]).assert().success();
    bl(bob.path()).args(["update", &t, "priority=3"]).assert().success();
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();
    bl(alice.path()).arg("sync").assert().success();
    let resolved = read_task_json(alice.path(), &t);
    let p = resolved["priority"].as_u64().unwrap();
    assert!(p == 2 || p == 3, "the resolver settles the same-field conflict: got {p}");
}
