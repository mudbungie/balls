//! bl-2bf7: state-branch sync on the **close** event — happy path
//! (archive propagates to origin) and the required-policy rollback
//! that keeps the worktree on an unreachable remote. The review-event
//! mirror stays in `lifecycle_sync.rs`; the shared remote fixtures
//! (`three_way`, `break_remote`, `write_some_code`) live in
//! `common::multidev`.

mod common;

use common::*;
use common::multidev::*;

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
