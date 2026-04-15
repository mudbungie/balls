//! Forward-compat round-trip tests for the lenient-serde contract:
//! older clients must preserve enum variants they don't recognize
//! instead of hard-erroring on the whole task file. Established by the
//! v0.3.0 `LinkType::Unknown` change (tests/gates_compat.rs) and
//! extended here to `Status`.

mod common;

use common::*;
use predicates::prelude::*;

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
