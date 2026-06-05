//! Gate tests — the §10 enforcement decision on a temp change worktree, driving
//! [`run`] with hand-built §7 payloads on a `Cursor`. File existence under
//! `tasks/` stands in for "unresolved" (§10).

use super::*;
use std::io::{self, Cursor};
use tempfile::tempdir;

/// A reader that always errors, to exercise the unreadable-payload path.
struct Failing;
impl Read for Failing {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("boom"))
    }
}

/// A §7 payload carrying just the blockers the gate reads.
fn payload(blockers: serde_json::Value) -> String {
    serde_json::json!({ "current_state": { "blockers": blockers } }).to_string()
}

fn write(dir: &Path, id: &str) {
    let tasks = dir.join("tasks");
    std::fs::create_dir_all(&tasks).unwrap();
    std::fs::write(tasks.join(format!("{id}.md")), "+++\ntitle = \"t\"\ncreated = 0\nupdated = 0\n+++\n").unwrap();
}

fn gate(op: &str, payload: &str, dir: &Path) -> i32 {
    run(&[op.into(), "pre".into()], &mut Cursor::new(payload.as_bytes().to_vec()), dir)
}

#[test]
fn protocol_self_describes_and_succeeds() {
    let d = tempdir().unwrap();
    assert_eq!(run(&["protocol".into()], &mut Cursor::new(Vec::new()), d.path()), 0);
}

#[test]
fn no_args_is_a_usage_error() {
    let d = tempdir().unwrap();
    assert_eq!(run(&[], &mut Cursor::new(Vec::new()), d.path()), 2);
}

#[test]
fn an_ungated_op_is_a_no_op() {
    let d = tempdir().unwrap();
    assert_eq!(gate("update", "", d.path()), 0);
}

#[test]
fn an_unreadable_payload_is_an_error() {
    let d = tempdir().unwrap();
    assert_eq!(run(&["claim".into(), "pre".into()], &mut Failing, d.path()), 2);
}

#[test]
fn a_malformed_payload_is_an_error() {
    let d = tempdir().unwrap();
    assert_eq!(gate("claim", "not json", d.path()), 2);
}

#[test]
fn an_absent_current_state_allows() {
    let d = tempdir().unwrap();
    assert_eq!(gate("claim", "{}", d.path()), 0);
}

#[test]
fn an_open_claim_blocker_blocks_claim() {
    let d = tempdir().unwrap();
    write(d.path(), "bl-dep");
    let p = payload(serde_json::json!([{ "id": "bl-dep", "on": "claim" }]));
    assert_eq!(gate("claim", &p, d.path()), 1);
}

#[test]
fn a_resolved_claim_blocker_allows_claim() {
    let d = tempdir().unwrap();
    // bl-dep's file does not exist ⇒ resolved.
    let p = payload(serde_json::json!([{ "id": "bl-dep", "on": "claim" }]));
    assert_eq!(gate("claim", &p, d.path()), 0);
}

#[test]
fn close_ignores_claim_blockers_and_checks_gates() {
    let d = tempdir().unwrap();
    write(d.path(), "bl-dep");
    write(d.path(), "bl-gate");
    let only_dep = payload(serde_json::json!([{ "id": "bl-dep", "on": "claim" }]));
    assert_eq!(gate("close", &only_dep, d.path()), 0); // claim-blocker irrelevant to close
    let with_gate = payload(serde_json::json!([{ "id": "bl-gate", "on": "close" }]));
    assert_eq!(gate("close", &with_gate, d.path()), 1);
}
