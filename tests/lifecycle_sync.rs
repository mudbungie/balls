//! bl-2bf7: state-branch sync on review and close.
//! Mirrors `tests/claim_sync.rs` for the new lifecycle events.

mod common;

use common::*;
use std::fs;

fn three_way() -> (Repo, Repo, Repo) {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    bl(alice.path()).arg("init").assert().success();
    push(alice.path());

    let bob = clone_from_remote(remote.path(), "bob");
    bl(bob.path()).arg("init").assert().success();
    (remote, alice, bob)
}

fn break_remote(repo: &std::path::Path) {
    git(repo, &["remote", "set-url", "origin", "/tmp/balls-no-such-remote.git"]);
}

fn write_some_code(wt: &std::path::Path, name: &str) {
    fs::write(wt.join(name), "code\n").unwrap();
}

#[test]
fn review_sync_happy_path_pushes_state_branch_close_to_origin() {
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "happy review");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = alice.path().join(".balls-worktrees").join(&id);
    write_some_code(&wt, "feature.txt");

    // --sync forces a remote round-trip on this review even with the
    // repo defaults left off.
    bl(alice.path())
        .args(["review", &id, "-m", "ready", "--sync"])
        .assert()
        .success();

    // Bob's sync picks up alice's review without any extra step.
    bl(bob.path()).arg("sync").assert().success();
    let j = read_task_json(bob.path(), &id);
    assert_eq!(j["status"], "review");
}

#[test]
fn close_sync_happy_path_pushes_state_branch_archive_to_origin() {
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "happy close");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = alice.path().join(".balls-worktrees").join(&id);
    write_some_code(&wt, "feature.txt");

    bl(alice.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();
    bl(alice.path())
        .args(["close", &id, "-m", "approved", "--sync"])
        .assert()
        .success();

    // Worktree is gone, claim file is gone, task archived locally.
    assert!(!wt.exists());
    assert!(!alice
        .path()
        .join(format!(".balls/tasks/{id}.json"))
        .exists());

    // Bob's sync sees the close.
    bl(bob.path()).arg("sync").assert().success();
    assert!(!bob
        .path()
        .join(format!(".balls/tasks/{id}.json"))
        .exists());
}

#[test]
fn review_sync_required_fails_loud_and_rolls_back_on_unreachable_remote() {
    // Required-policy rejection rolls back both squash and state commits.
    let (_r, alice, _bob) = three_way();
    let id = create_task(alice.path(), "doomed review");
    bl(alice.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = alice.path().join(".balls-worktrees").join(&id);
    write_some_code(&wt, "feature.txt");

    let pre_main = git(alice.path(), &["rev-parse", "HEAD"]).trim().to_string();

    break_remote(alice.path());
    let out = bl(alice.path())
        .args(["review", &id, "-m", "won't land", "--sync"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected review --sync to fail");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("review --sync") || stderr.contains("unreachable"),
        "stderr: {stderr}"
    );

    // Main's HEAD has rolled back to its pre-transition SHA.
    let post_main = git(alice.path(), &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(pre_main, post_main, "main should be rolled back");

    // Task is still in_progress, not review.
    let j = read_task_json(alice.path(), &id);
    assert_eq!(j["status"], "in_progress");
    assert!(j["delivered_in"].is_null());
}

#[test]
fn close_sync_required_fails_loud_and_keeps_worktree_on_unreachable_remote() {
    // Push runs *before* teardown so a rolled-back close keeps the
    // worktree, task file, and claim file intact for retry.
    let (_r, alice, _bob) = three_way();
    let id = create_task(alice.path(), "doomed close");
    bl(alice.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = alice.path().join(".balls-worktrees").join(&id);
    write_some_code(&wt, "feature.txt");
    bl(alice.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();

    break_remote(alice.path());
    let out = bl(alice.path())
        .args(["close", &id, "-m", "won't land", "--sync"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected close --sync to fail");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("close --sync") || stderr.contains("unreachable"),
        "stderr: {stderr}"
    );

    // Worktree, task file, and claim file all survive.
    assert!(wt.exists(), "worktree must survive a rolled-back close");
    assert!(alice
        .path()
        .join(format!(".balls/tasks/{id}.json"))
        .exists());
    assert!(alice
        .path()
        .join(".balls/local/claims")
        .join(&id)
        .exists());
    let j = read_task_json(alice.path(), &id);
    assert_eq!(j["status"], "review");
}

#[test]
fn no_sync_flag_skips_remote_on_review_and_close() {
    // --no-sync mirrors `bl claim --no-sync`: lifecycle proceeds local-only.
    let (_r, alice, _bob) = three_way();
    let id = create_task(alice.path(), "offline lifecycle");
    bl(alice.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = alice.path().join(".balls-worktrees").join(&id);
    write_some_code(&wt, "feature.txt");

    break_remote(alice.path());
    bl(alice.path())
        .args(["review", &id, "-m", "ok", "--no-sync"])
        .assert()
        .success();
    bl(alice.path())
        .args(["close", &id, "-m", "ok", "--no-sync"])
        .assert()
        .success();
}

#[test]
fn review_sync_retries_through_negotiation_when_state_branch_advanced() {
    // Concurrency: bob's claim of B lands on origin while alice is
    // mid-review of A. Alice's push hits non-FF; the negotiation
    // fetches, merges (different files), retries, succeeds.
    let (_r, alice, bob) = three_way();
    let id_a = create_task(alice.path(), "alice's review");
    let id_b = create_task(alice.path(), "bob's claim");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id_a])
        .assert()
        .success();
    let wt = alice.path().join(".balls-worktrees").join(&id_a);
    write_some_code(&wt, "from-alice.txt");

    // Bob claims task B with --sync; that lands on origin between
    // alice's local commits and her review push.
    bl_as(bob.path(), "bob")
        .args(["claim", &id_b, "--sync"])
        .assert()
        .success();

    // Alice's review --sync first push is non-fast-forward; the
    // negotiation primitive fetches bob's claim, merges (no conflict
    // — different task files), retries, and succeeds.
    bl(alice.path())
        .args(["review", &id_a, "-m", "concurrent ok", "--sync"])
        .assert()
        .success();

    let j = read_task_json(alice.path(), &id_a);
    assert_eq!(j["status"], "review");

    // Origin records both bob's claim of B and alice's review of A.
    let origin_log = git(alice.path(), &["log", "--format=%s", "origin/balls/tasks"]);
    assert!(
        origin_log
            .lines()
            .any(|l| l.contains(&format!("balls: claim {id_b}"))),
        "expected bob's claim on origin: {origin_log}"
    );
    assert!(
        origin_log
            .lines()
            .any(|l| l.contains(&format!("state: review {id_a}"))),
        "expected alice's review on origin: {origin_log}"
    );
}

#[test]
fn repo_default_require_remote_on_review_drives_review_to_remote() {
    // Repo default `require_remote_on_review=true` makes review
    // round-trip the remote without --sync, and break loud offline.
    let (_r, alice, _bob) = three_way();
    let cfg_path = alice.path().join(".balls/config.json");
    let cfg = fs::read_to_string(&cfg_path).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(&cfg).unwrap();
    j["require_remote_on_review"] = serde_json::Value::Bool(true);
    fs::write(&cfg_path, serde_json::to_string_pretty(&j).unwrap()).unwrap();
    git(alice.path(), &["add", ".balls/config.json"]);
    git(
        alice.path(),
        &["commit", "-m", "policy: require remote on review"],
    );

    let id = create_task(alice.path(), "policy review");
    bl(alice.path()).arg("sync").assert().success();
    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = alice.path().join(".balls-worktrees").join(&id);
    write_some_code(&wt, "feature.txt");

    break_remote(alice.path());
    let out = bl(alice.path())
        .args(["review", &id, "-m", "should fail"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "policy-driven review should fail");
    let post_status = read_task_json(alice.path(), &id);
    assert_eq!(post_status["status"], "in_progress");
}
