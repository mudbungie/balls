//! Init-related stories: 1, 2, 3, 73, 74, 75.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn story_1_init_in_existing_git_repo() {
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    assert!(repo.path().join(".balls/config.json").exists());
    assert!(repo.path().join(".balls/tasks").exists());
    assert!(repo.path().join(".balls/local/claims").exists());
    assert!(repo.path().join(".balls/local/lock").exists());
    let gi = std::fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gi.contains(".balls/local"));
    assert!(gi.contains(".balls-worktrees"));
    let log = git(repo.path(), &["log", "--oneline"]);
    assert!(log.contains("balls: initialize"));
}

#[test]
fn story_2_init_twice_is_idempotent() {
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    bl(repo.path()).arg("init").assert().success();
}

#[test]
fn story_3_init_in_cloned_repo_creates_local_only() {
    let remote = new_bare_remote();
    let dev_a = clone_from_remote(remote.path(), "alice");
    bl(dev_a.path()).arg("init").assert().success();
    push(dev_a.path());

    let _id = create_task(dev_a.path(), "task from A");
    push(dev_a.path());

    let dev_b = clone_from_remote(remote.path(), "bob");
    // Fresh clone has no .balls/tasks symlink and no .balls/local yet —
    // they're per-clone, gitignored, and materialized by `bl init`.
    assert!(!dev_b.path().join(".balls/tasks").exists());
    assert!(!dev_b.path().join(".balls/local").exists());
    bl(dev_b.path()).arg("init").assert().success();
    assert!(dev_b.path().join(".balls/local/claims").exists());
    assert!(dev_b.path().join(".balls/tasks").is_symlink());
    assert!(dev_b.path().join(".balls/worktree").exists());
}

#[test]
fn story_73_init_in_repo_with_no_commits() {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@t"]);
    git(dir.path(), &["config", "user.name", "t"]);
    bl(dir.path()).arg("init").assert().success();
    assert!(dir.path().join(".balls/config.json").exists());
}

#[test]
fn story_74_outside_git_repo() {
    let dir = tmp();
    bl(dir.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a git repository"));
}

#[test]
fn story_75_not_initialized() {
    let repo = new_repo();
    bl(repo.path())
        .args(["list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

#[test]
fn stealth_init_creates_external_tasks_dir() {
    let repo = new_repo();
    bl(repo.path())
        .args(["init", "--stealth"])
        .assert()
        .success()
        .stdout(predicate::str::contains("stealth"));
    // Tasks dir is outside the repo
    assert!(!repo.path().join(".balls/tasks").exists());
    // .balls/local/tasks_dir file exists with external path
    let td = std::fs::read_to_string(repo.path().join(".balls/local/tasks_dir")).unwrap();
    let ext = std::path::PathBuf::from(td.trim());
    assert!(ext.is_absolute());
    assert!(ext.exists());
}

#[test]
fn stealth_mode_full_lifecycle() {
    let repo = new_repo();
    bl(repo.path())
        .args(["init", "--stealth"])
        .assert()
        .success();
    let id = create_task(repo.path(), "stealth task");
    // Task exists in external dir, not in repo
    assert!(!repo.path().join(".balls/tasks").join(format!("{}.json", id)).exists());
    // List works
    let out = bl(repo.path()).arg("list").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("stealth task"));
    // Show works
    bl(repo.path())
        .args(["show", &id])
        .assert()
        .success();
    // Claim works
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    // Close works
    bl_as(repo.path(), "alice")
        .args(["close", &id])
        .assert()
        .success();
    // Task archived (external file deleted)
    let td = std::fs::read_to_string(repo.path().join(".balls/local/tasks_dir")).unwrap();
    let ext = std::path::PathBuf::from(td.trim());
    assert!(!ext.join(format!("{}.json", id)).exists());
    // No task commits in git log (stealth)
    let log = git(repo.path(), &["log", "--oneline"]);
    assert!(!log.contains(&format!("create {}", id)));
}
