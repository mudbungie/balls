//! §10 enforcement tests — the claim/close guards over a change worktree dir.
//! A blocker is "open" iff its `tasks/<id>.md` exists ([`touch`]); the guards
//! refuse with [`io::ErrorKind::PermissionDenied`] naming the open blockers.

use super::*;
use crate::task::Blocker;
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
