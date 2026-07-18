//! §10 enforcement tests — the claim/close guards over a change worktree dir.
//! A blocker is "open" iff its `tasks/<id>.md` exists ([`touch`]); the guards
//! refuse with [`io::ErrorKind::PermissionDenied`] naming the open blockers.

use super::*;
use crate::task::{Blocker, On};
use std::fs;
use tempfile::tempdir;

/// A bare task carrying just `blockers` — every other field is moot here.
fn task(blockers: Vec<Blocker>) -> Task {
    Task { blockers, ..Task::default() }
}

/// Mark a blocker OPEN: create its `tasks/<id>.md` so [`exists`] is true.
fn touch(dir: &Path, id: &str) {
    let tasks = dir.join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join(format!("{id}.md")), "").unwrap();
}

fn claim_blocker(id: &str) -> Blocker {
    Blocker { id: id.into(), on: On::Claim }
}

fn close_blocker(id: &str) -> Blocker {
    Blocker { id: id.into(), on: On::Close }
}

#[test]
fn claim_allows_a_task_with_no_blockers() {
    let d = tempdir().unwrap();
    claim(&task(vec![]), "bl-1", d.path()).unwrap();
}

#[test]
fn claim_is_blocked_by_an_open_dependency() {
    let d = tempdir().unwrap();
    touch(d.path(), "bl-dep"); // dep file present ⇒ unresolved
    let err = claim(&task(vec![claim_blocker("bl-dep")]), "bl-1", d.path()).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(err.to_string(), "claim: bl-1 blocked by unresolved bl-dep");
}

#[test]
fn claim_allows_once_the_dependency_resolves() {
    let d = tempdir().unwrap(); // bl-dep file absent ⇒ resolved
    claim(&task(vec![claim_blocker("bl-dep")]), "bl-1", d.path()).unwrap();
}

#[test]
fn a_close_blocker_does_not_gate_claim() {
    let d = tempdir().unwrap();
    touch(d.path(), "bl-gate"); // open, but it only gates close
    claim(&task(vec![close_blocker("bl-gate")]), "bl-1", d.path()).unwrap();
}

#[test]
fn close_allows_a_task_with_no_gates() {
    let d = tempdir().unwrap();
    close(&task(vec![]), "bl-1", d.path()).unwrap();
}

#[test]
fn close_is_blocked_by_an_open_gate() {
    let d = tempdir().unwrap();
    touch(d.path(), "bl-gate"); // gate child still open ⇒ unresolved
    let err = close(&task(vec![close_blocker("bl-gate")]), "bl-1", d.path()).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(err.to_string(), "close: bl-1 blocked by unresolved bl-gate");
}

#[test]
fn close_allows_once_the_gate_resolves() {
    let d = tempdir().unwrap(); // bl-gate file absent ⇒ resolved
    close(&task(vec![close_blocker("bl-gate")]), "bl-1", d.path()).unwrap();
}

#[test]
fn a_claim_blocker_does_not_gate_close() {
    let d = tempdir().unwrap();
    touch(d.path(), "bl-dep"); // open claim-blocker is moot at close
    close(&task(vec![claim_blocker("bl-dep")]), "bl-1", d.path()).unwrap();
}

#[test]
fn the_refusal_names_every_open_blocker() {
    let d = tempdir().unwrap();
    touch(d.path(), "bl-a");
    touch(d.path(), "bl-b");
    let blockers = vec![claim_blocker("bl-a"), claim_blocker("bl-b")];
    let err = claim(&task(blockers), "bl-1", d.path()).unwrap_err();
    assert_eq!(err.to_string(), "claim: bl-1 blocked by unresolved bl-a, bl-b");
}

#[test]
fn the_refusal_names_only_blockers_open_on_this_op() {
    // The message filter is the same `on == verb AND unresolved` conjunction the
    // gate applies: a RESOLVED claim blocker and an open CLOSE blocker are both
    // excluded from a claim refusal — only the blocker that actually gates shows.
    let d = tempdir().unwrap();
    touch(d.path(), "bl-open");
    touch(d.path(), "bl-gate"); // open, but it gates close, not this claim
    let blockers = vec![claim_blocker("bl-open"), claim_blocker("bl-done"), close_blocker("bl-gate")];
    let err = claim(&task(blockers), "bl-1", d.path()).unwrap_err();
    assert_eq!(err.to_string(), "claim: bl-1 blocked by unresolved bl-open");
}

/// A blocker on an op that is neither claim nor close.
fn op_blocker(id: &str, on: Verb) -> Blocker {
    Blocker { id: id.into(), on }
}

#[test]
fn gate_refuses_the_op_its_blocker_names() {
    // The generic op-keyed guard (§10/§15): an open on=update edge blocks update.
    let d = tempdir().unwrap();
    touch(d.path(), "bl-x");
    let err = gate(&task(vec![op_blocker("bl-x", Verb::Update)]), Verb::Update, "bl-1", d.path()).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(err.to_string(), "update: bl-1 blocked by unresolved bl-x");
}

#[test]
fn gate_ignores_a_blocker_naming_a_different_op() {
    let d = tempdir().unwrap();
    touch(d.path(), "bl-dep"); // open, but it gates claim, not unclaim
    gate(&task(vec![claim_blocker("bl-dep")]), Verb::Unclaim, "bl-1", d.path()).unwrap();
}

#[test]
fn gate_allows_once_the_blocker_resolves() {
    let d = tempdir().unwrap(); // bl-x absent ⇒ resolved
    gate(&task(vec![op_blocker("bl-x", Verb::Unclaim)]), Verb::Unclaim, "bl-1", d.path()).unwrap();
}

/// Write a REAL parseable ball carrying `blockers` — the acyclicity walk reads
/// files, unlike the in-hand guards above (an empty [`touch`] file fails parse
/// and reads as resolved).
fn ball(dir: &Path, id: &str, blockers: Vec<Blocker>) {
    crate::taskfile::write_task(dir, id, &task(blockers)).unwrap();
}

#[test]
fn acyclic_allows_the_one_edge_gate() {
    // The standard §10 topology: a gate close-blocks its parent, nothing waits
    // back — no loop, no refusal.
    let d = tempdir().unwrap();
    ball(d.path(), "bl-gate", vec![]);
    acyclic(d.path(), Verb::Create, "bl-work", &close_blocker("bl-gate")).unwrap();
}

#[test]
fn acyclic_refuses_the_lernie_two_edge_deadlock() {
    // bl-54fe's shape: the parent is already close-gated on the gate; adding
    // the gate's claim-blocker back on the parent closes the loop.
    let d = tempdir().unwrap();
    ball(d.path(), "bl-work", vec![close_blocker("bl-gate")]);
    let err = acyclic(d.path(), Verb::Update, "bl-gate", &claim_blocker("bl-work")).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    let msg = err.to_string();
    assert!(msg.contains("bl-gate -claim-> bl-work -close-> bl-gate"), "{msg}");
    assert!(msg.contains("bl-54fe"), "{msg}");
}

#[test]
fn acyclic_refuses_a_self_edge() {
    let d = tempdir().unwrap();
    ball(d.path(), "bl-a", vec![close_blocker("bl-a")]);
    let err = acyclic(d.path(), Verb::Update, "bl-a", &close_blocker("bl-a")).unwrap_err();
    assert!(err.to_string().contains("bl-a -close-> bl-a —"), "{err}");
}

#[test]
fn acyclic_names_every_hop_of_a_longer_loop() {
    // A -close-> B -claim-> C -close-> A: the refusal renders the whole walk.
    let d = tempdir().unwrap();
    ball(d.path(), "bl-b", vec![claim_blocker("bl-c")]);
    ball(d.path(), "bl-c", vec![close_blocker("bl-a")]);
    let err = acyclic(d.path(), Verb::Create, "bl-a", &close_blocker("bl-b")).unwrap_err();
    assert!(
        err.to_string().contains("bl-a -close-> bl-b -claim-> bl-c -close-> bl-a"),
        "{err}"
    );
}

#[test]
fn acyclic_ignores_edges_off_the_lifecycle() {
    let d = tempdir().unwrap();
    // The new edge itself gates a non-lifecycle op: never a resolution loop,
    // even though the reverse edge exists.
    ball(d.path(), "bl-b", vec![claim_blocker("bl-a")]);
    acyclic(d.path(), Verb::Update, "bl-a", &op_blocker("bl-b", Verb::Update)).unwrap();
    // A would-be loop routed through a non-lifecycle hop: B waits on C only
    // for update, so B still claims and closes — the chain is broken.
    ball(d.path(), "bl-b", vec![op_blocker("bl-c", Verb::Update)]);
    ball(d.path(), "bl-c", vec![close_blocker("bl-a")]);
    acyclic(d.path(), Verb::Update, "bl-a", &close_blocker("bl-b")).unwrap();
}

#[test]
fn acyclic_treats_an_absent_ball_as_resolved() {
    // The blocker names a ball with no file (already closed): resolved on
    // arrival, no live edges out — the walk stops there.
    let d = tempdir().unwrap();
    acyclic(d.path(), Verb::Update, "bl-a", &claim_blocker("bl-gone")).unwrap();
}

#[test]
fn a_preexisting_side_loop_never_refuses_an_unrelated_edge() {
    // B and C already deadlock each other (legacy wiring); A's new edge onto B
    // is not ON that loop, so it passes — and the seen-set keeps the walk from
    // circling B→C→B forever.
    let d = tempdir().unwrap();
    ball(d.path(), "bl-b", vec![claim_blocker("bl-c")]);
    ball(d.path(), "bl-c", vec![claim_blocker("bl-b")]);
    acyclic(d.path(), Verb::Update, "bl-a", &claim_blocker("bl-b")).unwrap();
}
