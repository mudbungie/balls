//! Review workflow: agent submits work, reviewer closes.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn review_merges_work_keeps_worktree() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();

    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("feature.txt"), "work").unwrap();

    bl(repo.path())
        .args(["review", &id, "-m", "ready for review"])
        .assert()
        .success();

    // Work merged to main
    assert!(repo.path().join("feature.txt").exists());
    // Worktree still exists
    assert!(wt.exists());
    // Task status is review, not closed
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "review");
    // Claim still active
    assert!(repo.path().join(".balls/local/claims").join(&id).exists());
}

#[test]
fn close_after_review_archives_and_removes_worktree() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("work.txt"), "done").unwrap();

    bl(repo.path())
        .args(["review", &id])
        .assert()
        .success();
    bl(repo.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();

    // Worktree removed
    assert!(!wt.exists());
    // Task archived
    assert!(!repo.path().join(format!(".balls/tasks/{id}.json")).exists());
    // Claim removed
    assert!(!repo.path().join(".balls/local/claims").join(&id).exists());
}

#[test]
fn close_rejects_from_inside_worktree() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);

    bl(repo.path())
        .args(["review", &id])
        .assert()
        .success();

    // Close from INSIDE the worktree — should be rejected
    bl(&wt)
        .args(["close", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot close from within the worktree"));
}

#[test]
fn review_as_attributes_note_to_override_identity() {
    // `bl review --as <id>` overrides BALLS_IDENTITY for note attribution.
    // Mirrors prime/claim/update so review is not a special case.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("a.txt"), "x").unwrap();

    bl_as(repo.path(), "ignored-by-as-flag")
        .args(["review", &id, "--as", "lernie", "-m", "via override"])
        .assert()
        .success();

    let notes = read_task_notes(repo.path(), &id);
    assert!(
        notes.iter().any(|n| n["author"] == "lernie"),
        "expected a note authored by 'lernie', got: {notes:?}"
    );
}

#[test]
fn close_as_overrides_identity() {
    // Same as the review case for the close path. The close commit
    // doesn't currently surface the identity, so we just assert the
    // command accepts the flag and succeeds — the plumbing is what
    // matters; future close-time attribution will use the same value.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("a.txt"), "x").unwrap();
    bl(repo.path())
        .args(["review", &id])
        .assert()
        .success();
    bl(repo.path())
        .args(["close", &id, "--as", "lernie", "-m", "approved by lernie"])
        .assert()
        .success();
}

#[test]
fn review_reject_back_to_in_progress() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("v1.txt"), "first attempt").unwrap();

    bl(repo.path())
        .args(["review", &id, "-m", "first try"])
        .assert()
        .success();

    // Reviewer rejects
    bl(repo.path())
        .args(["update", &id, "status=in_progress", "--note", "needs rework"])
        .assert()
        .success();

    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "in_progress");
    // Worktree still exists for rework
    assert!(wt.exists());

    // Agent continues working
    std::fs::write(wt.join("v2.txt"), "second attempt").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "second try"])
        .assert()
        .success();

    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "review");
    assert!(repo.path().join("v2.txt").exists());
}

#[test]
fn review_status_parse_and_display() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl(repo.path())
        .args(["update", &id, "status=review"])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "review");
}

// Squash/delivery edge cases moved to `review_squash.rs`.
