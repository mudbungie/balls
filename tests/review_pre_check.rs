//! bl-1f38 — the `review.pre_check` gate. `bl review` runs the
//! configured command against the post-merge worktree before it
//! delivers, and aborts the review (no squash, no push, no status
//! flip) when the command exits non-zero. End-to-end against the real
//! `bl` binary, in both delivery modes.

mod common;

use common::*;
use predicates::prelude::*;
use std::path::Path;

/// Set the review gate command in an already-initialized repo's
/// config. XDG repo.json names the field `review.gate_command` (the
/// renamed `pre_check`); the synthesizer maps it back to the legacy
/// `pre_check` for the load_config code path.
fn set_pre_check(repo: &Path, cmd: &str) {
    edit_and_commit_repo_config(repo, "review: set gate_command", |cfg| {
        cfg["review"] = serde_json::json!({ "gate_command": cmd });
    });
}

#[test]
fn passing_pre_check_allows_the_squash() {
    // A zero-exit gate is invisible: `bl review` squashes as normal.
    let repo = new_repo();
    init_in(repo.path());
    set_pre_check(repo.path(), "true");
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);
    std::fs::write(wt.join("feature.txt"), "work").unwrap();

    bl(repo.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();

    assert!(repo.path().join("feature.txt").exists());
    assert_eq!(read_task_json(repo.path(), &id)["status"], "review");
}

#[test]
fn failing_pre_check_aborts_then_a_fixed_retry_succeeds() {
    // A non-zero gate aborts with nothing delivered — status stays
    // in_progress, the integration branch is untouched, the worktree
    // survives — and the abort is clean enough that fixing the gate
    // and re-running `bl review` squashes normally.
    let repo = new_repo();
    init_in(repo.path());
    set_pre_check(repo.path(), "exit 1");
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);
    std::fs::write(wt.join("feature.txt"), "work").unwrap();

    bl(repo.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("review pre-check failed"));

    assert!(!repo.path().join("feature.txt").exists(), "squash leaked");
    assert_eq!(read_task_json(repo.path(), &id)["status"], "in_progress");
    assert!(wt.exists());

    set_pre_check(repo.path(), "true");
    bl(repo.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();
    assert!(repo.path().join("feature.txt").exists());
    assert_eq!(read_task_json(repo.path(), &id)["status"], "review");
}

#[test]
fn failing_pre_check_blocks_the_deferred_push() {
    // In deferred mode `bl review` pushes the work branch to a forge
    // instead of squashing. The gate runs before that push, so code
    // that fails it never reaches origin and no gate child is opened.
    let remote = new_bare_remote();
    let dev = clone_from_remote(remote.path(), "dev");
    bl(dev.path()).arg("init").assert().success();
    edit_and_commit_repo_config(dev.path(), "deferred mode + failing gate", |cfg| {
        cfg["integrate"] = serde_json::json!({ "mode": "forge-pr" });
        cfg["review"] = serde_json::json!({ "gate_command": "exit 1" });
    });
    git(dev.path(), &["push", "origin", "main"]);

    set_default_target_branch(Some("main".into()));
    let id = create_task(dev.path(), "feature");
    bl_as(dev.path(), "dev")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(dev.path(), &id);
    std::fs::write(wt.join("feature.txt"), "work").unwrap();

    bl(dev.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("review pre-check failed"));

    let refs = git(dev.path(), &["ls-remote", "origin"]);
    assert!(
        !refs.contains(&format!("work/{id}")),
        "work branch leaked to origin: {refs}"
    );
    assert_eq!(read_task_json(dev.path(), &id)["status"], "in_progress");
}
