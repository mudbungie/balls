//! Stealth-mode + `--tasks-dir` stories for Phase 1B (bl-e802) XDG
//! `bl init`. Stealth no longer writes anything under the clone's
//! `.balls/`; the pointer-and-overrides config moves to
//! `~/.config/balls/<nested-clone-path>/clone.json` (SPEC §4.1, §6.4).

mod common;

use balls::clone_json::CloneJson;
use balls::encoding::nested_clone_path;
use balls::xdg_paths::{clone_json_path, XdgBases};
use common::*;
use predicates::prelude::*;
use std::path::PathBuf;

fn test_bases() -> XdgBases {
    XdgBases::with(&test_home_path(), None, None, None)
}

fn stealth_clone_json(repo_root: &std::path::Path) -> CloneJson {
    let canon = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let nested = nested_clone_path(&canon);
    let p = clone_json_path(&test_bases(), &nested);
    CloneJson::read_optional(&p)
        .expect("clone.json read")
        .expect("clone.json present after stealth init")
}

#[test]
fn stealth_init_creates_external_tasks_dir() {
    let repo = new_repo();
    bl(repo.path())
        .args(["init", "--stealth"])
        .assert()
        .success()
        .stdout(predicate::str::contains("stealth"));
    // clone.json carries stealth + tasks_dir; tasks_dir defaults to the
    // cwd's `.balls/tasks/` per SPEC §4.1 (so the *default* stealth
    // path does land a `.balls/tasks/` under the cwd — by design, the
    // user opted into a colocated store with the flag).
    let cj = stealth_clone_json(repo.path());
    assert!(cj.stealth);
    let td = PathBuf::from(cj.tasks_dir.expect("tasks_dir set"));
    assert!(td.is_absolute(), "tasks_dir must be absolute: {}", td.display());
    assert!(td.exists(), "tasks_dir created: {}", td.display());
    // No `config.json` or `local/` siblings — only `tasks/` lands in
    // the tree, none of the pre-XDG control-file scaffolding.
    assert!(!repo.path().join(".balls/config.json").exists());
    assert!(!repo.path().join(".balls/local").exists());
}

#[test]
fn stealth_mode_full_lifecycle() {
    let repo = new_repo();
    bl(repo.path()).args(["init", "--stealth"]).assert().success();
    let id = create_task(repo.path(), "stealth task");
    // List works
    let out = bl(repo.path()).arg("list").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("stealth task"));
    // Show works
    bl(repo.path()).args(["show", &id]).assert().success();
    // Claim works (stealth uses --no-worktree by skill convention; the
    // stealth lifecycle here exercises the no-worktree path implicitly
    // because there is nothing under the worktree to symlink anyway).
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["close", &id, "-m", "shipped"])
        .assert()
        .success();
    // Task archived (the external task file is removed by close).
    let cj = stealth_clone_json(repo.path());
    let ext = PathBuf::from(cj.tasks_dir.expect("tasks_dir set"));
    assert!(!ext.join(format!("{id}.json")).exists());
    // No balls-attributed commits on main from any of the above.
    let log = git(repo.path(), &["log", "main", "--format=%s"]);
    for line in log.lines() {
        assert!(!line.starts_with("balls:"), "balls: commit on main: {line}");
    }
}

#[test]
fn custom_tasks_dir_stores_tasks_at_user_path() {
    let repo = new_repo();
    let custom = tempfile::Builder::new()
        .prefix("balls-custom-")
        .tempdir()
        .unwrap();
    let custom_path = custom.path().join("my-tasks");
    bl(repo.path())
        .args(["init", "--tasks-dir", custom_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("stealth"));
    assert!(custom_path.exists(), "custom tasks dir should be created");
    // SPEC §4.1: with --tasks-dir, the stealth clone's identity is the
    // tasks_dir path, so clone.json lives at
    // `~/.config/balls/<enc-tasks-dir>/clone.json`.
    let nested = nested_clone_path(&custom_path);
    let cj_path = clone_json_path(&test_bases(), &nested);
    let cj = CloneJson::read_optional(&cj_path)
        .expect("read")
        .expect("clone.json keyed by tasks_dir");
    assert!(cj.stealth);
    assert_eq!(cj.tasks_dir.as_deref(), custom_path.to_str());
    // End-to-end "task lands under custom tasks_dir" is the §14.15
    // conformance gate; this story focuses on the clone.json keying.
    // `bl create` from the original cwd would not find a clone.json
    // (the nested-path is the cwd, not the tasks_dir under §4.1), so
    // we exercise just the on-disk side here.
}

#[test]
fn tasks_dir_implies_stealth_without_flag() {
    let repo = new_repo();
    let custom = tempfile::Builder::new()
        .prefix("balls-impl-")
        .tempdir()
        .unwrap();
    let custom_path = custom.path().join("tasks");
    bl(repo.path())
        .args(["init", "--tasks-dir", custom_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("stealth"));
    assert!(!repo.path().join(".balls/tasks").exists());
}

#[test]
fn tasks_dir_rejects_relative_path() {
    let repo = new_repo();
    let out = bl(repo.path())
        .args(["init", "--tasks-dir", "relative/path"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("absolute"),
        "should reject relative path: {stderr}"
    );
}

#[test]
fn stealth_list_returns_empty_when_external_dir_gone() {
    let repo = new_repo();
    bl(repo.path()).args(["init", "--stealth"]).assert().success();
    // Read the external tasks dir out of clone.json, then blow it away
    // to simulate a user cleaning up /tmp.
    let cj = stealth_clone_json(repo.path());
    let ext = PathBuf::from(cj.tasks_dir.expect("tasks_dir"));
    let _ = std::fs::remove_dir_all(&ext);
    // `bl list` must succeed with empty output — not crash on the
    // missing dir inside Store::all_tasks.
    let out = bl(repo.path()).arg("list").output().unwrap();
    assert!(out.status.success(), "list should succeed: {out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).trim().is_empty());
}
