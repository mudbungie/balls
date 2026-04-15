//! Forward-compat round-trip tests for the lenient-serde contract:
//! older clients must preserve enum variants AND unknown top-level
//! fields they don't recognize instead of hard-erroring on the whole
//! task file. Established by the v0.3.0 `LinkType::Unknown` change
//! (tests/gates_compat.rs) and extended here to `Status`, `TaskType`,
//! and Task/Note struct passthrough.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn unknown_top_level_task_field_round_trips() {
    // Older bl must preserve unknown first-party top-level fields
    // across a save triggered by any mutation.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "future-field");

    let path = repo
        .path()
        .join(".balls/tasks")
        .join(format!("{id}.json"));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v.as_object_mut()
        .unwrap()
        .insert("severity".to_string(), serde_json::json!("sev2"));
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    // Mutation re-saves through serialize_mergeable.
    bl(repo.path())
        .args(["update", &id, "--note", "touching"])
        .assert()
        .success();
    let back: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(back["severity"], "sev2");
}

#[test]
fn unknown_task_type_round_trips_through_task_file() {
    // If a future version writes a task type we don't know (e.g.
    // "spike"), the whole task file must still load, `show` must
    // render it, and a save must preserve it.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "future-type");

    let path = repo
        .path()
        .join(".balls/tasks")
        .join(format!("{id}.json"));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["type"] = serde_json::json!("spike");
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    bl(repo.path())
        .args(["show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("spike"));

    bl(repo.path())
        .args(["update", &id, "--note", "touching"])
        .assert()
        .success();
    let back: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(back["type"], "spike");
}

#[test]
fn unknown_status_round_trips_through_task_file() {
    // If a future version writes a status we don't know, the whole
    // task file must still load, `show` must render it verbatim, and
    // a save (triggered here by a note append) must preserve it.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "future-status");

    let path = repo
        .path()
        .join(".balls/tasks")
        .join(format!("{id}.json"));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["status"] = serde_json::json!("triaged");
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    // show must not crash on the unknown status
    bl(repo.path())
        .args(["show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("triaged"));

    // A mutation re-saves the task file. The unknown status must survive.
    bl(repo.path())
        .args(["update", &id, "--note", "touching"])
        .assert()
        .success();
    let back: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(back["status"], "triaged");
}
