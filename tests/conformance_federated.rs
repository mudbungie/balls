//! SPEC-tracker-state §16 conformance — the federated topology.
//!
//! Test 11: federation routes task *state* to the tracker, never
//! code — `bl review`'s squash and `[bl-xxxx]` tag land on the
//! clone's own `origin`. Test 12: the merge-cleanliness gate,
//! run unconditionally in the two-participant topology.
//!
//! Fixtures hand-scaffold the XDG federated layout via
//! `common::xdg_init::xdg_federated_clone`. The legacy `state_url` in
//! `.balls/config.json` is gone in the XDG layout (SPEC §6.4 / §6.5);
//! the redirect lives in the `tracker.json` file on the code repo's
//! own `balls/tasks` branch (SPEC §5 / §6.1). The fixture materializes
//! both the own and federated tracker checkouts under the XDG bases
//! so `Store::discover`'s `xdg_discover::resolve_redirect` hop wires
//! through to the federated checkout.

mod common;

use common::tracker::*;
use common::xdg_init::{bl_xdg, xdg_federated_clone};
use common::*;
use std::path::Path;

/// Claim, deliver, and close one task from `ws` (origin = code remote).
/// Uses `bl()` so the subprocess inherits the per-thread test HOME —
/// matching whatever HOME was used to scaffold the federated layout.
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
    let home = test_home_path();
    let ws = xdg_federated_clone(
        &home,
        &url_of(&code),
        &url_of(&tracker),
        "ws",
    );

    let id = deliver(&ws, "feature work");
    bl(&ws).arg("sync").assert().success();

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
///
/// XDG keys the federated tracker checkout by `<enc-tracker-origin>`,
/// so two clones sharing one HOME would race on the same on-disk
/// state. Each dev therefore gets its own HOME tempdir; commands run
/// via `bl_xdg(path, home)` so the subprocess sees the dev-specific
/// XDG state, and reads of the merged JSON go through env-explicit
/// helpers below.
#[test]
fn t12_merge_cleanliness_two_participants() {
    let tracker = new_tracker();
    let tracker_url = url_of(&tracker);
    let code_a = new_bare_remote();
    let code_b = new_bare_remote();

    let home_a = tmp();
    let home_b = tmp();
    let alice = xdg_federated_clone(home_a.path(), &url_of(&code_a), &tracker_url, "alice");
    let bob = xdg_federated_clone(home_b.path(), &url_of(&code_b), &tracker_url, "bob");

    // Alice files a task; Bob picks it up off the tracker.
    let t = create_task_xdg(&alice, home_a.path(), "shared task");
    bl_xdg(&alice, home_a.path()).arg("sync").assert().success();
    bl_xdg(&bob, home_b.path()).arg("sync").assert().success();

    // Disjoint fields of the same task — must merge with no conflict.
    bl_xdg(&alice, home_a.path()).args(["update", &t, "priority=1"]).assert().success();
    bl_xdg(&alice, home_a.path()).arg("sync").assert().success();
    bl_xdg(&bob, home_b.path()).args(["update", &t, "description=bob-was-here"]).assert().success();
    bl_xdg(&bob, home_b.path()).arg("sync").assert().success();
    bl_xdg(&alice, home_a.path()).arg("sync").assert().success();
    let merged = read_xdg_task(&alice, home_a.path(), &t);
    assert_eq!(merged["priority"], 1, "Alice's disjoint edit survives");
    assert_eq!(merged["description"], "bob-was-here", "Bob's disjoint edit survives");

    // Concurrent note appends union-merge — both notes survive.
    bl_xdg(&alice, home_a.path())
        .args(["update", &t, "--note", "note-from-alice"])
        .assert()
        .success();
    bl_xdg(&bob, home_b.path())
        .args(["update", &t, "--note", "note-from-bob"])
        .assert()
        .success();
    bl_xdg(&alice, home_a.path()).arg("sync").assert().success();
    bl_xdg(&bob, home_b.path()).arg("sync").assert().success();
    bl_xdg(&alice, home_a.path()).arg("sync").assert().success();
    let notes = read_xdg_notes(&alice, home_a.path(), &t);
    let notes_json = serde_json::to_string(&notes).unwrap();
    assert!(notes_json.contains("note-from-alice"), "Alice's note survives: {notes_json}");
    assert!(notes_json.contains("note-from-bob"), "Bob's note survives: {notes_json}");

    // Same-field edit is a genuine conflict — the resolver settles it
    // deterministically rather than aborting the sync.
    bl_xdg(&alice, home_a.path()).args(["update", &t, "priority=2"]).assert().success();
    bl_xdg(&bob, home_b.path()).args(["update", &t, "priority=3"]).assert().success();
    bl_xdg(&alice, home_a.path()).arg("sync").assert().success();
    bl_xdg(&bob, home_b.path()).arg("sync").assert().success();
    bl_xdg(&alice, home_a.path()).arg("sync").assert().success();
    let resolved = read_xdg_task(&alice, home_a.path(), &t);
    let p = resolved["priority"].as_u64().unwrap();
    assert!(p == 2 || p == 3, "the resolver settles the same-field conflict: got {p}");
}

/// `bl create` in an explicit-HOME XDG context. Mirrors
/// `common::create_task` but routes through `bl_xdg`.
fn create_task_xdg(repo: &Path, home: &Path, title: &str) -> String {
    let out = bl_xdg(repo, home)
        .args(["create", title])
        .output()
        .expect("bl create");
    assert!(
        out.status.success(),
        "bl create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Read a task JSON directly from the federated tracker checkout for
/// `repo` under `home`. The XDG resolution lives in the binary; the
/// test mirrors it by asking `bl_xdg show --json` rather than
/// re-implementing the resolve_redirect hop.
///
/// `bl show --json` wraps the task body under a top-level `task`
/// key alongside dependency/delivery summaries; this helper unwraps
/// the inner object so callers can index by task field directly.
fn read_xdg_task(repo: &Path, home: &Path, id: &str) -> serde_json::Value {
    let out = bl_xdg(repo, home)
        .args(["show", id, "--json"])
        .output()
        .expect("bl show");
    assert!(
        out.status.success(),
        "bl show failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("show --json");
    v.get("task").cloned().unwrap_or(v)
}

/// Read a task's notes via `bl_xdg show --json`. The `notes` field is
/// a JSON array; return it as a `Vec<Value>` so the existing test
/// assertions can `to_string` it and substring-match.
fn read_xdg_notes(repo: &Path, home: &Path, id: &str) -> Vec<serde_json::Value> {
    let v = read_xdg_task(repo, home, id);
    v.get("notes")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default()
}
