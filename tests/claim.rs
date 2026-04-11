//! Claim stories: 22–32.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn story_22_claim_creates_worktree() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "implement feature");
    let out = bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .output()
        .unwrap();
    assert!(out.status.success());
    let wt = repo.path().join(".balls-worktrees").join(&id);
    assert!(wt.exists());
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "in_progress");
    assert_eq!(j["claimed_by"], "alice");
    assert_eq!(j["branch"], format!("work/{}", id));
    let claim = repo.path().join(".balls/local/claims").join(&id);
    assert!(claim.exists());
}

#[test]
fn story_23_double_claim_rejected() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    bl_as(repo.path(), "bob")
        .args(["claim", &id])
        .assert()
        .failure();
}

#[test]
fn story_24_claim_with_unmet_deps_rejected() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task_full(repo.path(), "b", 3, &[&a], &[]);
    bl(repo.path())
        .args(["claim", &b])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unmet"));
}

#[test]
fn story_25_claim_closed_task_rejected() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    // Closing via update archives the task (deletes the file)
    bl(repo.path())
        .args(["update", &a, "status=closed"])
        .assert()
        .success();
    // Archived task can't be claimed — it no longer exists
    bl(repo.path())
        .args(["claim", &a])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn story_27_worktree_has_local_cache_via_symlink() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt_local = repo
        .path()
        .join(".balls-worktrees")
        .join(&id)
        .join(".balls/local");
    assert!(wt_local.exists());
    let canon = std::fs::canonicalize(&wt_local).unwrap();
    let expected = std::fs::canonicalize(repo.path().join(".balls/local")).unwrap();
    assert_eq!(canon, expected);
}

#[test]
fn story_28_claim_with_identity() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl(repo.path())
        .args(["claim", &id, "--as", "dev1/agent-alpha"])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["claimed_by"], "dev1/agent-alpha");
}

#[test]
fn story_30_code_changes_in_worktree_isolated() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("feature.txt"), "my work").unwrap();
    assert!(!repo.path().join("feature.txt").exists());
}

#[test]
fn story_31_show_works_from_worktree() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    bl(&wt).args(["show", &id]).assert().success();
}

#[test]
fn story_32_update_note_from_worktree() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    bl(&wt)
        .args(["update", &id, "--note", "progress update"])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["notes"][0]["text"], "progress update");
}
