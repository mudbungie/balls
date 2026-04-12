//! Coverage tests for claim/close/drop/resolve edge cases.

mod common;

use common::*;

#[test]
fn resolve_file_that_doesnt_exist_errors() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["resolve", ".balls/tasks/nonexistent.json"])
        .assert()
        .failure();
}

#[test]
fn sync_without_remote_configured_succeeds() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "t");
    bl(repo.path()).arg("sync").assert().success();
}

#[test]
fn claim_nonexistent_task_errors() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["claim", "bl-0000"])
        .assert()
        .failure();
}

#[test]
fn close_nonexistent_task_errors() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["close", "bl-0000"])
        .assert()
        .failure();
}

#[test]
fn drop_nonexistent_task_errors() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["drop", "bl-0000"])
        .assert()
        .failure();
}

#[test]
fn repair_fix_on_clean_repo_is_noop() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path()).args(["repair", "--fix"]).assert().success();
}

#[test]
fn balls_identity_env_fallback() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "ident-test");
    bl(repo.path())
        .env("BALLS_IDENTITY", "env-agent")
        .args(["claim", &id])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["claimed_by"], "env-agent");
}

#[test]
fn claim_task_already_claimed_by_another_repo() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "remote-claimed");
    let task_path = repo.path().join(".balls/tasks").join(format!("{}.json", id));
    let mut j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&task_path).unwrap()).unwrap();
    j["claimed_by"] = serde_json::json!("remote-user");
    std::fs::write(&task_path, j.to_string()).unwrap();
    bl(repo.path())
        .args(["claim", &id])
        .assert()
        .failure()
        .stderr(predicates::prelude::predicate::str::contains("already claimed"));
}

#[test]
fn claim_rejected_when_worktree_dir_already_exists() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::create_dir_all(&wt).unwrap();
    bl(repo.path())
        .args(["claim", &id])
        .assert()
        .failure();
}

#[test]
fn claim_rolls_back_on_worktree_add_failure() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "rollback test");
    let branch = format!("work/{}", id);
    git(repo.path(), &["branch", &branch]);

    let out = bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .output()
        .unwrap();
    assert!(!out.status.success());

    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "open");
    assert!(j["claimed_by"].is_null());
    assert!(j["branch"].is_null());
    assert!(!repo.path().join(".balls/local/claims").join(&id).exists());
}

#[test]
fn claim_rejected_when_stale_claim_file_exists() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    let claim = repo.path().join(".balls/local/claims").join(&id);
    std::fs::create_dir_all(claim.parent().unwrap()).unwrap();
    std::fs::write(&claim, "worker=ghost\n").unwrap();
    bl(repo.path())
        .args(["claim", &id])
        .assert()
        .failure();
}

#[test]
fn sync_against_empty_remote_no_main_branch() {
    let remote = new_bare_remote();
    let dev = clone_from_remote(remote.path(), "dev");
    bl(dev.path()).arg("init").assert().success();
    create_task(dev.path(), "t");
    bl(dev.path()).arg("sync").assert().success();
}

#[test]
fn repair_removes_orphan_worktree() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let claim = repo.path().join(".balls/local/claims").join(&id);
    std::fs::remove_file(&claim).unwrap();

    let out = bl(repo.path())
        .args(["repair", "--fix"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("orphan worktree") || s.contains(&id));
}
