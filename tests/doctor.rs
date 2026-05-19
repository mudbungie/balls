//! `bl doctor` — read-only drift diagnostics. Every check, every
//! branch: a healthy repo is silent; each kind of drift names itself
//! and points at the fixing command without mutating anything.

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// Run `bl doctor` and return stdout. Asserts exit 0 — doctor is
/// read-only and never fails the process, the verdict is in the text.
fn doctor(cwd: &Path) -> String {
    let out = bl(cwd).arg("doctor").output().expect("bl doctor");
    assert!(out.status.success(), "doctor must exit 0 (read-only)");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn clean_repo_is_silent() {
    let repo = new_repo();
    init_in(repo.path());
    assert!(doctor(repo.path()).contains("no problems detected"));
}

#[test]
fn uninitialized_with_bl_docs_connects_them() {
    let dir = tmp();
    fs::write(dir.path().join("AGENTS.md"), "Task tracking uses bl prime.\n").unwrap();
    let out = doctor(dir.path());
    assert!(out.contains("bl is not usable here"));
    assert!(out.contains("docs reference bl"));
    assert!(out.contains("remove the bl"));
}

#[test]
fn uninitialized_without_docs_just_reports_discovery() {
    let dir = tmp();
    let out = doctor(dir.path());
    assert!(out.contains("bl is not usable here"));
    assert!(!out.contains("docs reference bl"));
    assert!(out.contains("1 problem(s)"));
}

#[test]
fn state_worktree_git_pointer_malformed() {
    let repo = new_repo();
    init_in(repo.path());
    fs::write(repo.path().join(".balls/worktree/.git"), "garbage\n").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("not a valid linked git worktree"));
    assert!(out.contains("bl repair"));
}

#[test]
fn state_worktree_git_pointer_missing() {
    let repo = new_repo();
    init_in(repo.path());
    fs::remove_file(repo.path().join(".balls/worktree/.git")).unwrap();
    assert!(doctor(repo.path()).contains("not a valid linked git worktree"));
}

#[test]
fn claim_file_with_no_task() {
    let repo = new_repo();
    init_in(repo.path());
    fs::write(repo.path().join(".balls/local/claims/bl-zzzz"), "x").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("claim file for bl-zzzz but no such task"));
    assert!(out.contains("bl repair --fix"));
}

#[test]
fn claim_file_for_task_not_in_progress() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "open task");
    fs::write(repo.path().join(".balls/local/claims").join(&id), "x").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains(&format!("claim file for {id} but its status is open")));
    assert!(out.contains("bl drop"));
}

#[test]
fn properly_claimed_task_is_silent() {
    // Exercises the in-progress claim arm and a worktree that *does*
    // have a matching claim — neither is drift.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "real work");
    bl(repo.path()).args(["claim", &id]).assert().success();
    assert!(doctor(repo.path()).contains("no problems detected"));
}

#[test]
fn orphan_worktree_dir_is_flagged() {
    let repo = new_repo();
    init_in(repo.path());
    // A worktree named after a real (but unclaimed) task is NOT an
    // orphan; one with no task or claim behind it is.
    let id = create_task(repo.path(), "has a task");
    let wt = repo.path().join(".balls-worktrees");
    fs::create_dir_all(wt.join(&id)).unwrap();
    fs::create_dir_all(wt.join("bl-dead")).unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("bl-dead"));
    assert!(out.contains("has no matching claim or task"));
    assert!(!out.contains(&format!("worktree dir {}", wt.join(&id).display())));
}

#[test]
fn corrupt_config_is_flagged() {
    let repo = new_repo();
    init_in(repo.path());
    fs::write(repo.path().join(".balls/config.json"), "{ not json").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("config") && out.contains("is unreadable"));
    assert!(out.contains("git checkout main"));
}

#[test]
fn tasks_dir_override_points_nowhere() {
    let repo = new_repo();
    init_in(repo.path());
    fs::write(
        repo.path().join(".balls/local/tasks_dir"),
        "/no/such/balls/path",
    )
    .unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("tasks_dir override"));
    assert!(out.contains("/no/such/balls/path"));
}

#[test]
fn healthy_stealth_store_is_silent() {
    // --tasks-dir points the override at a real directory: not drift,
    // and the state-worktree check is correctly skipped for stealth.
    let repo = new_repo();
    let ext = tmp();
    bl(repo.path())
        .args(["init", "--tasks-dir"])
        .arg(ext.path())
        .assert()
        .success();
    assert!(doctor(repo.path()).contains("no problems detected"));
}

#[test]
fn missing_claims_dir_is_not_an_error() {
    let repo = new_repo();
    init_in(repo.path());
    fs::remove_dir_all(repo.path().join(".balls/local/claims")).unwrap();
    assert!(doctor(repo.path()).contains("no problems detected"));
}
