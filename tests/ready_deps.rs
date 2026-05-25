//! Phase 2: ready queue and dependency management. Stories 17–21, 40–44.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn story_17_ready_queue_no_deps() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task_full(repo.path(), "alpha", 1, &[], &[]);
    let b = create_task_full(repo.path(), "beta", 2, &[], &[]);
    let out = bl(repo.path()).arg("ready").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let a_pos = s.find(&a).unwrap();
    let b_pos = s.find(&b).unwrap();
    assert!(a_pos < b_pos);
}

#[test]
fn story_18_ready_queue_with_deps() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "alpha");
    let b = create_task_full(repo.path(), "beta", 3, &[&a], &[]);
    let out = bl(repo.path()).arg("ready").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains(&a));
    assert!(!s.contains(&b)); // b is blocked by a
}

#[test]
fn story_19_ready_excludes_claimed() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "target");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let out = bl(repo.path()).arg("ready").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(!s.contains(&id));
}

#[test]
fn story_21_no_fetch_flag() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "a");
    bl(repo.path())
        .args(["ready", "--no-fetch"])
        .assert()
        .success();
}

#[test]
fn story_40_dep_add() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task(repo.path(), "b");
    bl(repo.path())
        .args(["dep", "add", &b, &a])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &b);
    assert_eq!(j["depends_on"][0], a);
}

#[test]
fn story_41_dep_cycle_rejected() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task_full(repo.path(), "b", 3, &[&a], &[]);
    bl(repo.path())
        .args(["dep", "add", &a, &b])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cycle"));
}

#[test]
fn story_42_dep_rm() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task_full(repo.path(), "b", 3, &[&a], &[]);
    bl(repo.path())
        .args(["dep", "rm", &b, &a])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &b);
    assert!(j["depends_on"].as_array().unwrap().is_empty());
}

#[test]
fn story_43_dep_tree_single_task() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task_full(repo.path(), "b", 3, &[&a], &[]);
    let out = bl(repo.path()).args(["dep", "tree", &b]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains(&a));
    assert!(s.contains(&b));
}

#[test]
fn story_44_dep_tree_full_graph() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task_full(repo.path(), "b", 3, &[&a], &[]);
    let out = bl(repo.path()).args(["dep", "tree"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    // b is a root (nothing depends on it)
    assert!(s.contains(&b));
    assert!(s.contains(&a));
}

#[test]
fn ready_queue_respects_transitive_deps() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "alpha");
    let b = create_task_full(repo.path(), "beta", 3, &[&a], &[]);
    let c = create_task_full(repo.path(), "gamma", 3, &[&b], &[]);

    // Close a — only b becomes ready
    bl(repo.path())
        .args(["update", &a, "status=closed"])
        .assert()
        .success();
    let out = bl(repo.path()).arg("ready").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains(&b));
    assert!(!s.contains(&c));
}

#[test]
fn ready_hides_parent_with_live_child() {
    // bl-c79c: a parent with an open child must not surface in `bl ready`
    // or in `bl prime --json`'s ready array — the child is the claimable
    // unit, the parent is not. Closing the last child re-exposes the
    // parent (its `bl close` is then the remaining action).
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "epic");
    let child_out = bl(repo.path())
        .args(["create", "kid", "--parent", &parent])
        .output()
        .unwrap();
    let child = String::from_utf8_lossy(&child_out.stdout).trim().to_string();

    let ready = bl(repo.path()).args(["ready", "--json"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&ready.stdout).unwrap();
    let ids: Vec<String> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    assert!(!ids.contains(&parent), "parent should be hidden: {ids:?}");
    assert!(ids.contains(&child), "child should be ready: {ids:?}");

    // Same filter must apply to `bl prime --json`'s ready array.
    let prime = bl_as(repo.path(), "agent")
        .args(["prime", "--json"])
        .output()
        .unwrap();
    let pv: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&prime.stdout).trim()).unwrap();
    let pids: Vec<String> = pv["ready"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, pids, "prime ready must match ready --json");

    // Close the child — parent now has no live children and reappears.
    bl(repo.path())
        .args(["update", &child, "status=closed"])
        .assert()
        .success();
    let ready = bl(repo.path()).args(["ready", "--json"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&ready.stdout).unwrap();
    let ids: Vec<String> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&parent), "parent should reappear: {ids:?}");
}

#[test]
fn claim_rejects_parent_with_live_child() {
    // bl-c79c: claiming a parent directly by id must fail with a message
    // naming a live child, so a shell wrapper that passes through a
    // parent id doesn't silently misroute the work.
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "epic");
    let child_out = bl(repo.path())
        .args(["create", "kid", "--parent", &parent])
        .output()
        .unwrap();
    let child = String::from_utf8_lossy(&child_out.stdout).trim().to_string();
    bl_as(repo.path(), "agent")
        .args(["claim", &parent])
        .assert()
        .failure()
        .stderr(predicate::str::contains(&child));
}

#[test]
fn ready_json_output() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "a");
    let out = bl(repo.path()).args(["ready", "--json"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v.is_array());
    assert_eq!(v.as_array().unwrap().len(), 1);
}
