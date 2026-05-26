//! Typed links: add/remove, show rendering, and the closed-target
//! rules. Split out of `ready_deps.rs` — link management accreted onto
//! that file but is a distinct concern from the ready queue.

mod common;

use common::*;
use predicates::prelude::*;

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

/// Drive a task through review and close so it survives only in
/// state-branch history (the recovery path `bl show <id>` already
/// uses). Returns the closed task's id.
fn closed_task(repo_path: &std::path::Path, title: &str) -> String {
    let id = create_task(repo_path, title);
    bl(repo_path).args(["claim", &id]).assert().success();
    let wt = worktree_path(repo_path, &id);
    std::fs::write(wt.join("feature.txt"), title).unwrap();
    bl(repo_path)
        .args(["review", &id, "-m", "ship"])
        .assert()
        .success();
    bl(repo_path)
        .args(["close", &id, "-m", "ok"])
        .assert()
        .success();
    id
}

#[test]
fn link_add_accepts_closed_target_for_nongates_types() {
    // Retrospective cross-refs (relates_to / duplicates / supersedes /
    // replies_to) must succeed against a just-closed ball: the lookup
    // falls back to archive recovery so a follow-up ball can record
    // the link on its own (open) side. See bl-3983.
    let repo = new_repo();
    init_in(repo.path());
    let closed = closed_task(repo.path(), "predecessor");
    let a = create_task(repo.path(), "follow-up");
    for lt in ["relates_to", "duplicates", "supersedes", "replies_to"] {
        bl(repo.path())
            .args(["link", "add", &a, lt, &closed])
            .assert()
            .success();
    }
    let j = read_task_json(repo.path(), &a);
    let kinds: Vec<&str> = j["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["link_type"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"relates_to"));
    assert!(kinds.contains(&"duplicates"));
    assert!(kinds.contains(&"supersedes"));
    assert!(kinds.contains(&"replies_to"));
}

#[test]
fn link_add_rejects_closed_target_for_gates() {
    // `gates` blocks close until the target closes — a target that is
    // already closed is a no-op blocker and would let bl close pass
    // spuriously. Stays rejected.
    let repo = new_repo();
    init_in(repo.path());
    let closed = closed_task(repo.path(), "audited");
    let a = create_task(repo.path(), "auditor");
    bl(repo.path())
        .args(["link", "add", &a, "gates", &closed])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
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
