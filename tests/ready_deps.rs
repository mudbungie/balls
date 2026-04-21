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


#[test]
fn link_add_and_show() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task(repo.path(), "b");
    bl(repo.path())
        .args(["link", "add", &a, "relates_to", &b])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &a);
    let links = j["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["link_type"], "relates_to");
    assert_eq!(links[0]["target"], b);
    // Show displays links
    let out = bl(repo.path()).args(["show", &a]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("relates_to"));
}

#[test]
fn link_rm() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task(repo.path(), "b");
    bl(repo.path())
        .args(["link", "add", &a, "duplicates", &b])
        .assert()
        .success();
    bl(repo.path())
        .args(["link", "rm", &a, "duplicates", &b])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &a);
    assert!(j["links"].as_array().unwrap().is_empty());
}

#[test]
fn link_add_nonexistent_target_fails() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    bl(repo.path())
        .args(["link", "add", &a, "relates_to", "bl-0000"])
        .assert()
        .failure();
}

#[test]
fn link_add_bad_type_fails() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task(repo.path(), "b");
    bl(repo.path())
        .args(["link", "add", &a, "bogus", &b])
        .assert()
        .failure();
}

#[test]
fn link_add_idempotent() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task(repo.path(), "b");
    bl(repo.path())
        .args(["link", "add", &a, "supersedes", &b])
        .assert()
        .success();
    bl(repo.path())
        .args(["link", "add", &a, "supersedes", &b])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &a);
    assert_eq!(j["links"].as_array().unwrap().len(), 1);
}
