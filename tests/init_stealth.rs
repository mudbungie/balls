//! Stealth mode and `--tasks-dir` stories, split from `init.rs` to
//! keep both test binaries under the 300-line cap. Covers the
//! external/custom tasks-dir layout and the stealth lifecycle.

mod common;

use common::*;
use predicates::prelude::*;

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
    assert!(!repo.path().join(".balls/tasks").join(format!("{id}.json")).exists());
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
    assert!(!ext.join(format!("{id}.json")).exists());
    // No task commits in git log (stealth)
    let log = git(repo.path(), &["log", "--oneline"]);
    assert!(!log.contains(&format!("create {id}")));
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
    let td = std::fs::read_to_string(repo.path().join(".balls/local/tasks_dir")).unwrap();
    assert_eq!(td.trim(), custom_path.to_str().unwrap());
    let id = create_task(repo.path(), "custom dir task");
    assert!(
        custom_path.join(format!("{id}.json")).exists(),
        "task should be in custom dir"
    );
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
fn stealth_init_gitignores_code_refs_cache() {
    // `.balls/code-refs` is materialized by `--resolve-remote`
    // regardless of mode, so stealth init must gitignore it too —
    // the non-stealth-only `.balls/tasks`/`.balls/worktree` gating
    // does not apply to it.
    let repo = new_repo();
    bl(repo.path())
        .args(["init", "--stealth"])
        .assert()
        .success();
    let gi = std::fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gi.contains(".balls/code-refs"), "stealth .gitignore: {gi}");
}

#[test]
fn stealth_list_returns_empty_when_external_dir_gone() {
    let repo = new_repo();
    bl(repo.path())
        .args(["init", "--stealth"])
        .assert()
        .success();
    // Read the external tasks dir out of the stealth pointer file,
    // then blow it away to simulate a user cleaning up /tmp.
    let td = std::fs::read_to_string(repo.path().join(".balls/local/tasks_dir")).unwrap();
    let ext = std::path::PathBuf::from(td.trim());
    let _ = std::fs::remove_dir_all(&ext);
    // `bl list` must succeed with empty output — not crash on the
    // missing dir inside Store::all_tasks.
    let out = bl(repo.path()).arg("list").output().unwrap();
    assert!(out.status.success(), "list should succeed: {out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).trim().is_empty());
}
