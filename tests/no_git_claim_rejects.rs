//! No-git `bl claim --no-worktree` rejection paths split out of
//! `tests/no_git.rs` to keep both files under the 300-line cap. The
//! shared stealth-init helper lives in this file to avoid the cross-
//! test-binary cycle (helpers in `tests/common/` are visible from
//! every integration binary that does `mod common;`).

mod common;

use common::*;
use predicates::prelude::*;
use std::path::PathBuf;

/// Same shape as `tests/no_git.rs::init_no_git`: stand up a no-git
/// stealth store whose tasks_dir is the cwd itself so subsequent
/// `bl(dir.path())` calls resolve via the same clone.json.
fn init_no_git() -> (tempfile::TempDir, PathBuf) {
    let dir = tmp();
    let tasks_path = std::fs::canonicalize(dir.path()).unwrap();
    bl(&tasks_path)
        .args(["init", "--tasks-dir", tasks_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("stealth"));
    (dir, tasks_path)
}

#[test]
fn claim_no_worktree_rejects_nonexistent_task() {
    let (dir, _p) = init_no_git();
    bl(dir.path())
        .args(["claim", "bl-0000", "--no-worktree"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn claim_no_worktree_rejects_already_claimed() {
    // Manufacture an inconsistent state: status=open but claimed_by set.
    let (dir, _p) = init_no_git();
    let id = create_task(dir.path(), "claim dup");
    bl(dir.path()).args(["claim", &id, "--no-worktree"]).assert().success();
    bl(dir.path()).args(["update", &id, "status=open"]).assert().success();
    bl(dir.path())
        .args(["claim", &id, "--no-worktree"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already claimed"));
}

#[test]
fn claim_no_worktree_rejects_dep_blocked_task() {
    let (dir, _p) = init_no_git();
    let a = create_task(dir.path(), "blocker");
    let out = bl(dir.path())
        .args(["create", "blocked", "--dep", &a])
        .output()
        .unwrap();
    let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
    bl(dir.path())
        .args(["claim", &b, "--no-worktree"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unmet"));
}

#[test]
fn claim_no_worktree_rejects_parent_with_live_child() {
    // bl-c79c: the parent-has-live-children guard fires in the
    // `--no-worktree` claim path too, naming a live child.
    let (dir, _p) = init_no_git();
    let parent = create_task(dir.path(), "epic");
    let out = bl(dir.path())
        .args(["create", "kid", "--parent", &parent])
        .output()
        .unwrap();
    let child = String::from_utf8_lossy(&out.stdout).trim().to_string();
    bl(dir.path())
        .args(["claim", &parent, "--no-worktree"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(&child));
}

#[test]
fn no_worktree_claim_works_in_git_mode_too() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "git but no wt");
    bl(repo.path())
        .args(["claim", &id, "--no-worktree"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no worktree"));
    let out = bl(repo.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["task"]["status"].as_str().unwrap(), "in_progress");
}
