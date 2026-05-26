//! Review-vs-main merge behavior, split from `close_drop.rs`: review
//! merges an advanced main into the worktree first, and surfaces a
//! genuine content conflict as a failure.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn review_merges_main_into_worktree_first() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "first");
    let b = create_task(repo.path(), "second");

    // Task A: claim, work, review, close (advances main)
    bl_as(repo.path(), "alice").args(["claim", &a]).assert().success();
    let wt_a = worktree_path(repo.path(), &a);
    std::fs::write(wt_a.join("file_a.txt"), "from task a").unwrap();
    bl(repo.path()).args(["review", &a]).assert().success();
    bl(repo.path()).args(["close", &a]).assert().success();

    // Task B: claim, work, review succeeds despite main divergence
    bl_as(repo.path(), "bob").args(["claim", &b]).assert().success();
    let wt_b = worktree_path(repo.path(), &b);
    std::fs::write(wt_b.join("file_b.txt"), "from task b").unwrap();
    bl(repo.path()).args(["review", &b]).assert().success();
    bl(repo.path()).args(["close", &b]).assert().success();

    assert!(repo.path().join("file_a.txt").exists());
    assert!(repo.path().join("file_b.txt").exists());
}

#[test]
fn review_detects_conflict_with_main() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "first");
    let b = create_task(repo.path(), "second");
    bl_as(repo.path(), "alice").args(["claim", &a]).assert().success();
    bl_as(repo.path(), "bob").args(["claim", &b]).assert().success();
    let wt_a = worktree_path(repo.path(), &a);
    let wt_b = worktree_path(repo.path(), &b);
    std::fs::write(wt_a.join("shared.txt"), "version A").unwrap();
    git(wt_a.as_path(), &["add", "shared.txt"]);
    git(wt_a.as_path(), &["commit", "-m", "A", "--no-verify"]);
    std::fs::write(wt_b.join("shared.txt"), "version B").unwrap();
    git(wt_b.as_path(), &["add", "shared.txt"]);
    git(wt_b.as_path(), &["commit", "-m", "B", "--no-verify"]);
    // Review A succeeds, review B fails — conflict on merge
    bl(repo.path()).args(["review", &a]).assert().success();
    bl(repo.path()).args(["close", &a]).assert().success();
    bl(repo.path()).args(["review", &b]).assert().failure()
        .stderr(predicate::str::contains("conflict"));
}
