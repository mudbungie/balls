//! Forward- and backward-compat tests for the gates feature:
//! unknown link variants, malformed gate children, and pre-gates
//! task files. Split from tests/gates.rs to keep each file under
//! the 300-line cap.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn unknown_link_variant_round_trips_through_task_file() {
    // Forward-compat guarantee: if a future version writes a link
    // variant we don't know, we must preserve it through a load/save
    // cycle instead of hard-erroring on the whole task file.
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let child = create_task(repo.path(), "other");

    // Hand-craft a task file with a future link type.
    let path = repo
        .path()
        .join(".balls/tasks")
        .join(format!("{parent}.json"));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["links"] = serde_json::json!([
        { "link_type": "from_the_future", "target": child }
    ]);
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    // `bl show` must not crash.
    bl(repo.path())
        .args(["show", &parent])
        .assert()
        .success()
        .stdout(predicate::str::contains("from_the_future"));

    // Round-trip: another mutation (add a note) re-saves the file.
    // The unknown link must still be present afterward.
    bl(repo.path())
        .args(["update", &parent, "--note", "touching"])
        .assert()
        .success();
    let back: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let links = back["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["link_type"], "from_the_future");
    assert_eq!(links[0]["target"], child);
}

#[test]
fn malformed_gate_child_propagates_load_error() {
    // If a gate-linked child exists but its JSON file is corrupted,
    // the close must fail loudly (not silently treat the gate as
    // satisfied). This exercises the defensive error arm in
    // open_gate_blockers.
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let child = create_task(repo.path(), "audit");
    bl(repo.path())
        .args(["link", "add", &parent, "gates", &child])
        .assert()
        .success();

    let child_path = repo
        .path()
        .join(".balls/tasks")
        .join(format!("{child}.json"));
    std::fs::write(&child_path, "{ not valid json").unwrap();

    bl(repo.path())
        .args(["update", &parent, "status=closed"])
        .assert()
        .failure();
}

#[test]
fn pre_gates_task_file_parses_unchanged() {
    // Back-compat guard: a task file written before gates existed
    // (no `links` array at all, or empty) must load cleanly. The
    // 0.3.0 release ships the forward-compat serde change, but
    // backward-compat to prior on-disk shapes is non-negotiable.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "legacy");
    let path = repo
        .path()
        .join(".balls/tasks")
        .join(format!("{id}.json"));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    // Strip the links field entirely, mimicking a pre-gates write.
    let obj = v.as_object_mut().unwrap();
    obj.remove("links");
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    // Show should succeed and report zero links.
    bl(repo.path())
        .args(["show", &id])
        .assert()
        .success();
    // Any mutation must re-save without inventing spurious content.
    bl(repo.path())
        .args(["update", &id, "--note", "touch"])
        .assert()
        .success();
    let back: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let links = back["links"].as_array().unwrap();
    assert!(
        links.is_empty(),
        "pre-gates file should round-trip with empty links"
    );
}

#[test]
fn unknown_link_does_not_block_close() {
    // An unknown link type is NOT a gate — only the `gates` variant
    // blocks close. This guards against a future bug where any non-
    // known variant accidentally gets treated as blocking.
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let other = create_task(repo.path(), "other");
    let path = repo
        .path()
        .join(".balls/tasks")
        .join(format!("{parent}.json"));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["links"] = serde_json::json!([
        { "link_type": "from_the_future", "target": other }
    ]);
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    bl(repo.path())
        .args(["update", &parent, "status=closed"])
        .assert()
        .success();
}
