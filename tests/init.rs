//! Init-related stories: 1, 2, 3, 73, 74, 75.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn story_1_init_in_existing_git_repo() {
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    assert!(repo.path().join(".ball/config.json").exists());
    assert!(repo.path().join(".ball/tasks").exists());
    assert!(repo.path().join(".ball/local/claims").exists());
    assert!(repo.path().join(".ball/local/lock").exists());
    let gi = std::fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gi.contains(".ball/local"));
    assert!(gi.contains(".ball-worktrees"));
    let log = git(repo.path(), &["log", "--oneline"]);
    assert!(log.contains("ball: initialize"));
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
    assert!(dev_b.path().join(".ball/tasks").exists());
    assert!(!dev_b.path().join(".ball/local").exists());
    bl(dev_b.path()).arg("init").assert().success();
    assert!(dev_b.path().join(".ball/local/claims").exists());
}

#[test]
fn story_73_init_in_repo_with_no_commits() {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@t"]);
    git(dir.path(), &["config", "user.name", "t"]);
    bl(dir.path()).arg("init").assert().success();
    assert!(dir.path().join(".ball/config.json").exists());
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
