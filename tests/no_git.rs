//! No-git mode: balls works without a git repository when using
//! --tasks-dir. Claim requires --no-worktree; review/close auto-detect.

mod common;

use common::*;
use predicates::prelude::*;
use std::path::PathBuf;

fn init_no_git() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let dir = tmp();
    let tasks_tmp = tmp();
    let tasks_path = tasks_tmp.path().join("tasks");
    bl(dir.path())
        .args(["init", "--tasks-dir", tasks_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("stealth"));
    (dir, tasks_tmp, tasks_path)
}

#[test]
fn init_outside_git_with_tasks_dir_succeeds() {
    let (dir, _tasks_tmp, tasks_path) = init_no_git();
    assert!(tasks_path.exists());
    assert!(dir.path().join(".balls/config.json").exists());
}

#[test]
fn init_outside_git_without_tasks_dir_fails() {
    let dir = tmp();
    bl(dir.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a git repository"));
}

#[test]
fn init_stealth_without_tasks_dir_outside_git_fails() {
    let dir = tmp();
    bl(dir.path())
        .args(["init", "--stealth"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a git repository"));
}

#[test]
fn create_and_list_in_no_git_store() {
    let (dir, _t, _p) = init_no_git();
    let id = create_task(dir.path(), "no git task");
    let out = bl(dir.path()).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("no git task"), "list should show task: {stdout}");
    bl(dir.path()).args(["show", &id]).assert().success();
}

#[test]
fn claim_without_no_worktree_flag_errors_in_no_git() {
    let (dir, _t, _p) = init_no_git();
    let id = create_task(dir.path(), "need flag");
    bl(dir.path())
        .args(["claim", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--no-worktree"));
}

#[test]
fn claim_no_worktree_succeeds() {
    let (dir, _t, _p) = init_no_git();
    let id = create_task(dir.path(), "claimable");
    bl(dir.path())
        .args(["claim", &id, "--no-worktree"])
        .assert()
        .success()
        .stdout(predicate::str::contains("claimed").and(predicate::str::contains("no worktree")));
    let out = bl(dir.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse show json");
    assert_eq!(v["task"]["status"].as_str().unwrap(), "in_progress");
    assert!(v["task"]["claimed_by"].as_str().is_some());
}

#[test]
fn full_lifecycle_no_git() {
    let (dir, _t, tasks_path) = init_no_git();
    let id = create_task(dir.path(), "lifecycle task");

    bl(dir.path())
        .args(["claim", &id, "--no-worktree"])
        .assert()
        .success();

    bl(dir.path())
        .args(["review", &id, "-m", "done"])
        .assert()
        .success();
    let out = bl(dir.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["task"]["status"].as_str().unwrap(), "review");

    bl(dir.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();
    assert!(
        !tasks_path.join(format!("{id}.json")).exists(),
        "task file should be deleted on close"
    );
}

#[test]
fn drop_in_no_git_mode() {
    let (dir, _t, _p) = init_no_git();
    let id = create_task(dir.path(), "to drop");
    bl(dir.path())
        .args(["claim", &id, "--no-worktree"])
        .assert()
        .success();
    bl(dir.path())
        .args(["drop", &id])
        .assert()
        .success();
    let out = bl(dir.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["task"]["status"].as_str().unwrap(), "open");
}

#[test]
fn sync_in_no_git_mode_succeeds() {
    let (dir, _t, _p) = init_no_git();
    bl(dir.path())
        .arg("sync")
        .assert()
        .success()
        .stderr(predicate::str::contains("sync complete"));
}

#[test]
fn ready_in_no_git_mode() {
    let (dir, _t, _p) = init_no_git();
    create_task(dir.path(), "ready task");
    bl(dir.path())
        .arg("ready")
        .assert()
        .success()
        .stdout(predicate::str::contains("ready task"));
}

#[test]
fn repair_in_no_git_mode() {
    let (dir, _t, _p) = init_no_git();
    bl(dir.path())
        .args(["repair", "--fix"])
        .assert()
        .success();
}

#[test]
fn close_no_git_without_message() {
    let (dir, _t, tasks_path) = init_no_git();
    let id = create_task(dir.path(), "close no msg");
    bl(dir.path()).args(["claim", &id, "--no-worktree"]).assert().success();
    bl(dir.path()).args(["review", &id]).assert().success();
    bl(dir.path()).args(["close", &id]).assert().success();
    assert!(!tasks_path.join(format!("{id}.json")).exists());
}

#[test]
fn claim_no_worktree_rejects_non_open_task() {
    let (dir, _t, _p) = init_no_git();
    let id = create_task(dir.path(), "already claimed");
    bl(dir.path()).args(["claim", &id, "--no-worktree"]).assert().success();
    let out = bl(dir.path())
        .args(["claim", &id, "--no-worktree"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn discover_non_balls_dir_fails() {
    let dir = tmp();
    bl(dir.path())
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn discover_balls_dir_without_stealth_tasks_fails() {
    // .balls/config.json exists but no tasks_dir pointer — not a valid
    // no-git store (you need --tasks-dir to be a no-git store).
    let dir = tmp();
    std::fs::create_dir_all(dir.path().join(".balls")).unwrap();
    std::fs::write(
        dir.path().join(".balls/config.json"),
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}"#,
    )
    .unwrap();
    bl(dir.path())
        .arg("list")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn claim_no_worktree_rejects_nonexistent_task() {
    let (dir, _t, _p) = init_no_git();
    bl(dir.path())
        .args(["claim", "bl-0000", "--no-worktree"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn claim_no_worktree_rejects_already_claimed() {
    // Manufacture an inconsistent state: status=open but claimed_by set.
    let (dir, _t, _p) = init_no_git();
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
    let (dir, _t, _p) = init_no_git();
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
