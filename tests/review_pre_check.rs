//! bl-1f38 — the `review.pre_check` gate. `bl review` runs the
//! configured command against the post-merge worktree before it
//! delivers, and aborts the review (no squash, no push, no status
//! flip) when the command exits non-zero. End-to-end against the real
//! `bl` binary, in both delivery modes.

mod common;

use common::*;
use predicates::prelude::*;
use std::path::Path;

/// Set `review.pre_check` in an already-initialized repo's config.
fn set_pre_check(repo: &Path, cmd: &str) {
    let p = repo.join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    cfg["review"] = serde_json::json!({ "pre_check": cmd });
    std::fs::write(&p, cfg.to_string()).unwrap();
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
    let wt = repo.path().join(".balls-worktrees").join(&id);
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
    let wt = repo.path().join(".balls-worktrees").join(&id);
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
    let cfg = dev.path().join(".balls/config.json");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(
        &cfg,
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees","target_branch":"main","delivery":{"mode":"deferred"},"review":{"pre_check":"exit 1"}}"#,
    )
    .unwrap();
    bl(dev.path()).arg("init").assert().success();
    git(dev.path(), &["push", "origin", "main"]);

    let id = create_task(dev.path(), "feature");
    bl_as(dev.path(), "dev")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = dev.path().join(".balls-worktrees").join(&id);
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
