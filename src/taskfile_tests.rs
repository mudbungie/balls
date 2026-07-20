//! `taskfile` tests for the two primitives unique to this module — `exists`
//! (the §10 resolver) and `add_blocker` (the front-door reciprocal edge). The
//! read/write/`task_ids` helpers are exercised through `change`'s base-change
//! tests.

use super::*;
use crate::task::On;
use tempfile::tempdir;

const TASK: &str = "+++\ntitle = \"A task\"\ncreated = 0\nupdated = 0\n+++\nbody\n";

fn write(dir: &Path, id: &str, md: &str) {
    let tasks = dir.join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join(format!("{id}.md")), md).unwrap();
}

#[test]
fn exists_tracks_the_task_file() {
    let d = tempdir().unwrap();
    let dir = d.path();
    assert!(!exists(dir, "bl-1"));
    write(dir, "bl-1", TASK);
    assert!(exists(dir, "bl-1"));
}

#[test]
fn add_blocker_appends_the_edge_and_bumps_updated() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    add_blocker(dir, "bl-1", Blocker { id: "bl-2".into(), on: On::Close }, 42).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert_eq!(t.blockers, vec![Blocker { id: "bl-2".into(), on: On::Close }]);
    assert_eq!(t.updated, 42);
}

#[test]
fn add_blocker_is_idempotent() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let edge = Blocker { id: "bl-2".into(), on: On::Claim };
    add_blocker(dir, "bl-1", edge.clone(), 1).unwrap();
    add_blocker(dir, "bl-1", edge.clone(), 2).unwrap();
    assert_eq!(read_task(dir, "bl-1").unwrap().blockers, vec![edge]);
}

#[test]
fn add_blocker_errors_when_the_target_is_absent() {
    let d = tempdir().unwrap();
    let err = add_blocker(d.path(), "bl-gone", Blocker { id: "bl-2".into(), on: On::Claim }, 0)
        .unwrap_err();
    assert!(err.to_string().contains("no such open ball: bl-gone"), "{err}");
}

#[test]
fn read_task_refuses_a_missing_ball_in_balls_voice() {
    // A resolved (closed) or never-was ball has no `tasks/<id>.md`; absence is
    // the record, so the read refuses in voice, never leaking the raw errno.
    let d = tempdir().unwrap();
    let err = read_task(d.path(), "bl-gone").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("no such open ball: bl-gone"), "{msg}");
    assert!(msg.contains("absence is the record"), "{msg}");
    assert!(!msg.contains("os error"), "no raw errno: {msg}");
}

#[test]
fn read_task_maps_a_malformed_ball_to_invalid_data() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", "not frontmatter");
    assert_eq!(read_task(dir, "bl-1").unwrap_err().kind(), io::ErrorKind::InvalidData);
}
