//! §9 base-change tests — each verb's authoring (stage) and §5 message
//! (finalize) on a plain temp dir; finalize asserts the `bl-op` trailer via
//! [`crate::message::parse`], proving the lifecycle seam each verb fills.

use super::*;
use crate::message::parse;
use crate::task::{Blocker, On, Task};
use tempfile::tempdir;

const TASK: &str = "+++\ntitle = \"A task\"\ncreated = 0\nupdated = 0\n+++\nbody\n";
const CLAIMED: &str = "+++\ntitle = \"A task\"\ncreated = 0\nupdated = 0\nclaimant = \"bob\"\n+++\n";
const RICH: &str = "+++\ntitle = \"A task\"\ncreated = 0\nupdated = 0\nparent = \"bl-old\"\n\
priority = 1\ntags = [\"a\"]\n\n[[blockers]]\nid = \"bl-z\"\non = \"claim\"\n+++\nbody\n";

fn write(dir: &Path, id: &str, md: &str) {
    let tasks = dir.join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(tasks.join(format!("{id}.md")), md).unwrap();
}

#[test]
fn claim_sets_the_claimant_and_bumps_updated() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let o = Occupancy::claim("bl-1".into(), "alice".into(), 1_749_085_200);
    o.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert_eq!(t.claimant.as_deref(), Some("alice"));
    assert_eq!(t.updated, 1_749_085_200);
    let md = parse(&o.finalize(dir).unwrap()).unwrap();
    assert_eq!(md["bl-op"], ["claim"]);
    assert_eq!(md["bl-id"], ["bl-1"]);
}

#[test]
fn claim_refuses_an_already_claimed_ball() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", CLAIMED);
    let o = Occupancy::claim("bl-1".into(), "alice".into(), 0);
    let err = o.stage(dir).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    assert!(err.to_string().contains("already claimed by bob"));
}

#[test]
fn unclaim_clears_the_claimant() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", CLAIMED);
    let o = Occupancy::unclaim("bl-1".into(), "alice".into(), 22);
    o.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    assert!(t.claimant.is_none());
    assert_eq!(t.updated, 22);
    let md = parse(&o.finalize(dir).unwrap()).unwrap();
    assert_eq!(md["bl-op"], ["unclaim"]);
}

#[test]
fn guard_repo_rejects_only_a_definite_cross_repo_mismatch() {
    // bl-1ce7: the wrong-repo claim guard fires ONLY when the ball's recorded
    // root and this checkout's root are BOTH present and DIFFER; every other
    // shape passes (back-compat / fail-open, no override).
    let with = |r: Option<&str>| Task { root_commit: r.map(str::to_string), ..Task::default() };
    // Both present, equal → the same project → pass.
    super::guard_repo(&with(Some("aaa")), Some("aaa"), "bl-1").unwrap();
    // Both present, differ → reject, naming BOTH roots so the message points at
    // the right checkout (identity is remote-free — no path, no remote).
    let err = super::guard_repo(&with(Some("aaa")), Some("bbb"), "bl-1").unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    let msg = err.to_string();
    assert!(msg.contains("rooted at aaa") && msg.contains("rooted at bbb"), "{msg}");
    // No recorded root (pre-feature ball, or born off no code repo) → unconstrained.
    super::guard_repo(&with(None), Some("bbb"), "bl-1").unwrap();
    // No current root (claim off a checkout with no code repo) → unprovable → pass.
    super::guard_repo(&with(Some("aaa")), None, "bl-1").unwrap();
}

#[test]
fn claim_stage_rejects_a_wrong_repo_ball() {
    // The guard is wired into `claim`'s stage: a ball recorded against root
    // `aaa`, claimed from a checkout rooted at `bbb`, is refused before the seal.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", "+++\ntitle = \"A\"\ncreated = 0\nupdated = 0\nroot_commit = \"aaa\"\n+++\n");
    let mut o = Occupancy::claim("bl-1".into(), "me".into(), 0);
    o.current_root = Some("bbb".into());
    let err = o.stage(dir).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    // The matching-root claim seals (the ball is recorded against this checkout).
    o.current_root = Some("aaa".into());
    o.stage(dir).unwrap();
    assert_eq!(read_task(dir, "bl-1").unwrap().claimant.as_deref(), Some("me"));
}

#[test]
fn close_removes_the_file_and_emits_a_close_message() {
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let r = Retire::close("bl-1".into(), "A task".into(), "me".into());
    r.stage(dir).unwrap();
    assert!(!task_path(dir, "bl-1").exists());
    let msg = r.finalize(dir).unwrap();
    assert!(msg.starts_with("A task"));
    let md = parse(&msg).unwrap();
    assert_eq!(md["bl-op"], ["close"]);
    assert_eq!(md["bl-id"], ["bl-1"]);
}

#[test]
fn retire_errors_when_the_ball_is_absent() {
    let d = tempdir().unwrap();
    let r = Retire::close("bl-gone".into(), "t".into(), "me".into());
    assert!(r.stage(d.path()).is_err());
}

#[test]
fn the_m_message_flows_into_the_commit_body_under_the_title_subject() {
    // The subject is ALWAYS the ball title (no override); `-m` is the free body.
    let d = tempdir().unwrap();
    let dir = d.path();
    write(dir, "bl-1", TASK);
    let mut o = Occupancy::claim("bl-1".into(), "me".into(), 0);
    o.message = Some("Extra paragraph.".into());
    o.stage(dir).unwrap();
    let msg = o.finalize(dir).unwrap();
    assert!(msg.starts_with("A task"));
    assert!(msg.contains("Extra paragraph."));
}

// The `create` authoring tests share this module's `write`/`TASK` fixtures.
#[path = "change_create_tests.rs"]
mod create;

// The `update` authoring tests share this module's `write`/`TASK`/`RICH` fixtures.
#[path = "change_field_tests.rs"]
mod field;
