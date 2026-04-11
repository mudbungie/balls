//! Phase 6: agent lifecycle — prime, full agent loop. Stories 59–63.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn story_59_prime_outputs_ready_queue() {
    let repo = new_repo();
    init_in(repo.path());
    create_task_full(repo.path(), "first", 1, &[], &[]);
    create_task_full(repo.path(), "second", 2, &[], &[]);

    let out = bl_as(repo.path(), "agent-alpha")
        .arg("prime")
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("agent-alpha"));
    assert!(s.contains("first"));
    assert!(s.contains("second"));
}

#[test]
fn story_60_61_full_agent_loop() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task_full(repo.path(), "top priority", 1, &[], &[]);
    create_task_full(repo.path(), "next", 2, &[], &[]);

    // Agent primes, claims top ready, writes work, closes
    let _ = bl_as(repo.path(), "agent-alpha").arg("prime").output();
    bl_as(repo.path(), "agent-alpha")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("work.txt"), "done").unwrap();
    bl_as(repo.path(), "agent-alpha")
        .args(["close", &id, "-m", "shipped"])
        .assert()
        .success();

    // After close, prime should show the "next" task as ready
    let out = bl_as(repo.path(), "agent-alpha")
        .arg("prime")
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("next"));
}

#[test]
fn story_63_agent_crashes_task_stays_in_progress() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "running");
    bl_as(repo.path(), "agent-alpha")
        .args(["claim", &id])
        .assert()
        .success();
    // Simulate crash: just check that task is still in_progress and
    // a human supervisor can drop it.
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "in_progress");
    // Even a different identity can drop it
    bl_as(repo.path(), "supervisor")
        .args(["drop", &id])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "open");
}

#[test]
fn repair_reports_ok_when_clean() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "ok");
    let out = bl(repo.path()).arg("repair").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("OK"));
}

#[test]
fn repair_fix_removes_orphan_claim() {
    let repo = new_repo();
    init_in(repo.path());
    // Fabricate an orphan claim file
    std::fs::write(
        repo.path().join(".balls/local/claims/bl-ghost"),
        "worker=ghost\npid=1\nclaimed_at=2026-01-01T00:00:00Z\n",
    )
    .unwrap();
    let out = bl(repo.path())
        .args(["repair", "--fix"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("bl-ghost"));
    assert!(!repo.path().join(".balls/local/claims/bl-ghost").exists());
}

#[test]
fn update_multiple_fields() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl(repo.path())
        .args([
            "update",
            &id,
            "priority=1",
            "status=blocked",
            "description=new desc",
            "title=new title",
            "type=bug",
        ])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["priority"], 1);
    assert_eq!(j["status"], "blocked");
    assert_eq!(j["description"], "new desc");
    assert_eq!(j["title"], "new title");
    assert_eq!(j["type"], "bug");
}

#[test]
fn update_unknown_field_errors() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl(repo.path())
        .args(["update", &id, "bogus=xyz"])
        .assert()
        .failure();
}

#[test]
fn update_bad_priority_errors() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl(repo.path())
        .args(["update", &id, "priority=9"])
        .assert()
        .failure();
}

#[test]
fn create_bad_priority_rejected() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["create", "x", "-p", "9"])
        .assert()
        .failure();
}

#[test]
fn create_bad_type_rejected() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["create", "x", "-t", "bogus"])
        .assert()
        .failure();
}

#[test]
fn create_bad_parent_rejected() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["create", "x", "--parent", "bl-0000"])
        .assert()
        .failure();
}

#[test]
fn dep_add_missing_target_errors() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    bl(repo.path())
        .args(["dep", "add", &a, "bl-xxxx"])
        .assert()
        .failure();
}

#[test]
fn show_nonexistent_task_errors() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["show", "bl-0000"])
        .assert()
        .failure();
}

#[test]
fn update_status_closed_rejects_claimed_task() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "claimed");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &id, "status=closed"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("bl close"));
}

#[test]
fn update_status_closed_archives_unclaimed_task() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "unclaimed");
    bl(repo.path())
        .args(["update", &id, "status=closed", "--note", "done without worktree"])
        .assert()
        .success();
    // Task file is archived (deleted from HEAD)
    let task_path = repo.path().join(format!(".balls/tasks/{}.json", id));
    assert!(!task_path.exists());
    // Git log shows the close (close+archive combined)
    let log = git(repo.path(), &["log", "--oneline"]);
    assert!(log.contains(&format!("close {}", id)));
}
