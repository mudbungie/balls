//! No-git mode: balls works without a git repository when using
//! --tasks-dir. Claim requires --no-worktree; review/close auto-detect.

mod common;

use common::*;
use predicates::prelude::*;
use std::path::PathBuf;

/// Stand up a no-git stealth store and return `(holding tempdir,
/// unused 2nd tempdir kept for return-shape compatibility, tasks_dir
/// path)`. Under XDG SPEC §4.1 the stealth clone's identity is the
/// `--tasks-dir`, so subsequent `bl(dir.path())` calls only resolve
/// when `dir.path() == tasks_dir`; the helper uses the parent tempdir
/// itself as the tasks_dir so the legacy "run bl from `dir`" pattern
/// keeps working.
fn init_no_git() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
    let dir = tmp();
    let tasks_path = std::fs::canonicalize(dir.path()).unwrap();
    bl(&tasks_path)
        .args(["init", "--tasks-dir", tasks_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("stealth"));
    let tasks_tmp = tmp(); // unused, kept for tuple-shape compat
    (dir, tasks_tmp, tasks_path)
}

#[test]
fn init_outside_git_with_tasks_dir_succeeds() {
    let (_dir, _tasks_tmp, tasks_path) = init_no_git();
    assert!(tasks_path.exists());
    // SPEC §4.1: XDG stealth init writes `clone.json` keyed by the
    // tasks_dir under `~/.config/balls/<nested>/clone.json`; no
    // `.balls/` at the clone root. The presence of the clone.json
    // file is the load-bearing post-condition.
    let bases = balls::xdg_paths::XdgBases::with(&test_home_path(), None, None, None);
    let nested = balls::encoding::nested_clone_path(&tasks_path);
    let cj = balls::xdg_paths::clone_json_path(&bases, &nested);
    assert!(cj.exists(), "clone.json at {}", cj.display());
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
fn init_stealth_without_tasks_dir_outside_git_defaults_to_cwd_tasks() {
    // XDG SPEC §4.1: `bl init --stealth` without `--tasks-dir`
    // outside a git repo defaults `tasks_dir` to `<cwd>/.balls/tasks`
    // and writes `clone.json` keyed by the cwd — succeeds without a
    // git checkout.
    let dir = tmp();
    let cwd = std::fs::canonicalize(dir.path()).unwrap();
    bl(&cwd)
        .args(["init", "--stealth"])
        .assert()
        .success()
        .stdout(predicate::str::contains("stealth"));
    assert!(cwd.join(".balls/tasks").is_dir());
    let bases = balls::xdg_paths::XdgBases::with(&test_home_path(), None, None, None);
    let nested = balls::encoding::nested_clone_path(&cwd);
    assert!(balls::xdg_paths::clone_json_path(&bases, &nested).exists());
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

