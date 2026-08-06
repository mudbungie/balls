//! `reopen`'s base change on a plain temp dir: it writes the INJECTED task back
//! (restamping only `updated`), renders the §5 `bl-op: reopen` trailer, and runs
//! the §10 gate for its own op like every other mutating verb.

use std::fs;
use std::path::Path;

use super::Reopen;
use crate::lifecycle::BaseChange;
use crate::message::parse;
use crate::task::{Blocker, Task};
use crate::taskfile::read_task;
use crate::verb::Verb;
use tempfile::tempdir;

/// A ball as history would hand it back: claimed, with the shaping fields set.
fn dead() -> Task {
    Task {
        title: "A retired ball".into(),
        created: 100,
        updated: 200,
        claimant: Some("ghost".into()),
        priority: Some(3),
        tags: vec!["bug".into()],
        body: "the body".into(),
        ..Task::default()
    }
}

fn change(task: Task) -> Reopen {
    Reopen { id: "bl-1".into(), task, actor: "alice".into(), now: 900, message: None }
}

fn tasks_dir(dir: &Path) {
    fs::create_dir_all(dir.join("tasks")).unwrap();
}

#[test]
fn reopen_writes_the_injected_task_back_verbatim_but_restamps_updated() {
    let d = tempdir().unwrap();
    let dir = d.path();
    tasks_dir(dir);
    let c = change(dead());
    c.stage(dir).unwrap();
    let t = read_task(dir, "bl-1").unwrap();
    // Everything the ball died with survives — only `updated` moves.
    assert_eq!(t.title, "A retired ball");
    assert_eq!(t.created, 100);
    assert_eq!(t.claimant.as_deref(), Some("ghost"));
    assert_eq!(t.priority, Some(3));
    assert_eq!(t.tags, ["bug"]);
    assert_eq!(t.body, "the body");
    assert_eq!(t.updated, 900);
}

#[test]
fn reopen_renders_the_op_trailer_from_the_restored_title() {
    let d = tempdir().unwrap();
    let dir = d.path();
    tasks_dir(dir);
    let c = change(dead());
    c.stage(dir).unwrap();
    let md = parse(&c.finalize(dir).unwrap()).unwrap();
    assert_eq!(md["bl-op"], ["reopen"]);
    assert_eq!(md["bl-id"], ["bl-1"]);
    assert_eq!(md["bl-actor"], ["alice"]);
}

#[test]
fn reopen_carries_the_m_note_into_the_commit_body() {
    let d = tempdir().unwrap();
    let dir = d.path();
    tasks_dir(dir);
    let mut c = change(dead());
    c.message = Some("picking it back up".into());
    c.stage(dir).unwrap();
    assert!(c.finalize(dir).unwrap().contains("picking it back up"));
}

#[test]
fn reopen_is_gated_by_an_unresolved_blocker_naming_its_own_op() {
    let d = tempdir().unwrap();
    let dir = d.path();
    tasks_dir(dir);
    // The gate resolves by file existence, so a LIVE blocker file refuses.
    fs::write(dir.join("tasks/bl-gate.md"), "+++\ntitle = \"g\"\ncreated = 0\nupdated = 0\n+++\n").unwrap();
    let mut task = dead();
    task.blockers = vec![Blocker { id: "bl-gate".into(), on: Verb::Reopen }];
    let err = change(task).stage(dir).unwrap_err();
    assert!(err.to_string().contains("bl-gate"), "{err}");
    assert!(!dir.join("tasks/bl-1.md").exists(), "a refused reopen writes nothing");
}

#[test]
fn a_blocker_naming_another_op_does_not_gate_reopen() {
    let d = tempdir().unwrap();
    let dir = d.path();
    tasks_dir(dir);
    fs::write(dir.join("tasks/bl-gate.md"), "+++\ntitle = \"g\"\ncreated = 0\nupdated = 0\n+++\n").unwrap();
    let mut task = dead();
    task.blockers = vec![Blocker { id: "bl-gate".into(), on: Verb::Close }];
    change(task).stage(dir).unwrap();
    assert!(dir.join("tasks/bl-1.md").exists());
}
