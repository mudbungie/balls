//! SPEC §7.4 conformance: half-push detection.
//!
//! A half-push is a task whose close commit landed on the state branch
//! but whose corresponding `[bl-xxxx]` delivery tag is not reachable
//! from main. `bl sync` must surface this on read so the main push
//! can be retried (or the state rolled back on a different machine).

mod common;

use common::*;

/// Drive a full review → close lifecycle, then rewind main so its tip
/// no longer contains the `[bl-xxxx]` tag. The state branch still
/// holds both the review and close commits. Sync must warn.
#[test]
fn sync_warns_when_main_is_behind_state_close() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "half-pushed feature");

    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("feature.txt"), "content").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();
    bl(repo.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();

    // At this point: state branch has `state: review bl-XXX` and
    // `state: close bl-XXX`; main has the feature commit `... [bl-XXX]`.
    // Simulate the half-push by hard-resetting main past the feature
    // commit, leaving state branch ahead.
    git(repo.path(), &["reset", "--hard", "HEAD~1"]);

    // bl sync has no remote here, so sync_with_remote is a no-op for
    // the push path — but detect_half_push is exercised separately via
    // cmd_sync's no-remote-warning flow. Call it explicitly through a
    // syncless bl sync: we pass a remote name that doesn't exist so
    // remote sync is skipped, then invoke sync again with the
    // detection path active.
    //
    // Approach: point origin at a bare remote so has_remote is true,
    // then run sync. The state branch push will succeed (the remote
    // is empty), the main push will succeed (main is reset), and the
    // detector will then compare state subjects against main subjects
    // and flag bl-XXX.
    let remote = new_bare_remote();
    git(
        repo.path(),
        &[
            "remote",
            "add",
            "origin",
            remote.path().to_str().unwrap(),
        ],
    );

    let out = bl(repo.path()).arg("sync").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "sync should succeed even with a half-push: {}",
        stderr
    );
    assert!(
        stderr.contains(&format!("state branch records close for {}", id)),
        "expected half-push warning for {}, got: {}",
        id,
        stderr
    );
}

/// Tasks closed via `bl update status=closed` without ever being
/// reviewed must NOT be flagged: those legitimately have no main
/// commit.
#[test]
fn update_closed_without_review_is_not_a_half_push() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "unclaimed close");
    bl(repo.path())
        .args(["update", &id, "status=closed"])
        .assert()
        .success();

    let remote = new_bare_remote();
    git(
        repo.path(),
        &[
            "remote",
            "add",
            "origin",
            remote.path().to_str().unwrap(),
        ],
    );
    let out = bl(repo.path()).arg("sync").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !stderr.contains("state branch records close"),
        "update status=closed must not trigger half-push warning: {}",
        stderr
    );
}
