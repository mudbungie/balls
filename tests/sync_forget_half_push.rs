//! SPEC §7.4 conformance: half-push retraction via `bl repair
//! --forget-half-push` / `--forget-all-half-pushes`.

mod common;

use common::*;

/// `bl repair --forget-half-push <id>` retracts a stale warning by
/// committing a `state: forget-half-push <id>` marker on the state
/// branch. After the retraction, subsequent syncs must not re-flag it.
#[test]
fn forget_half_push_suppresses_subsequent_warning() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "legacy half-pushed");
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
    git(repo.path(), &["reset", "--hard", "HEAD~1"]);

    let remote = new_bare_remote();
    git(
        repo.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );

    // Precondition: sync flags the half-push.
    let out = bl(repo.path()).arg("sync").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains(&format!("state branch records close for {id}")),
        "precondition: expected half-push warning for {id}: {stderr}"
    );

    // Retract the warning.
    let out = bl(repo.path())
        .args(["repair", "--forget-half-push", &id])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "forget-half-push failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains(&format!("forgot half-push: {id}")),
        "expected confirmation for {id}: {stdout}"
    );

    // Subsequent sync must be quiet.
    let out = bl(repo.path()).arg("sync").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !stderr.contains(&format!("state branch records close for {id}")),
        "post-forget sync should not re-flag {id}: {stderr}"
    );
}

/// `--forget-half-push <id>` on an id that isn't currently flagged is
/// rejected with a clear error. Prevents accidental marker commits on
/// unrelated tasks.
#[test]
fn forget_half_push_rejects_unknown_id() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "live task");

    let out = bl(repo.path())
        .args(["repair", "--forget-half-push", &id])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "forget-half-push on a non-flagged id must fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains(&format!("{id} is not a currently-flagged half-push")),
        "expected reason in error: {stderr}"
    );
}

/// `--forget-all-half-pushes` retracts every currently-flagged id in
/// one go. Common cleanup path for repos carrying stale pre-0.3.8
/// gate-review warnings.
#[test]
fn forget_all_half_pushes_clears_every_warning() {
    let repo = new_repo();
    init_in(repo.path());
    let id_a = create_task(repo.path(), "half-pushed A");
    let id_b = create_task(repo.path(), "half-pushed B");
    for id in [&id_a, &id_b] {
        bl_as(repo.path(), "alice")
            .args(["claim", id])
            .assert()
            .success();
        let wt = repo.path().join(".balls-worktrees").join(id);
        std::fs::write(wt.join(format!("{id}.txt")), "c").unwrap();
        bl(repo.path())
            .args(["review", id, "-m", "ready"])
            .assert()
            .success();
        bl(repo.path())
            .args(["close", id, "-m", "ok"])
            .assert()
            .success();
    }
    // Rewind past both feature commits so neither [bl-xxxx] tag is
    // reachable from main.
    git(repo.path(), &["reset", "--hard", "HEAD~2"]);

    let remote = new_bare_remote();
    git(
        repo.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );

    let out = bl(repo.path()).arg("sync").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains(&id_a) && stderr.contains(&id_b),
        "precondition: both ids should warn: {stderr}"
    );

    let out = bl(repo.path())
        .args(["repair", "--forget-all-half-pushes"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "forget-all failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains(&id_a) && stdout.contains(&id_b),
        "expected both ids in output: {stdout}"
    );

    let out = bl(repo.path()).arg("sync").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !stderr.contains("state branch records close"),
        "post-forget-all sync should be quiet: {stderr}"
    );
}

/// Retraction requires a git-backed, non-stealth repo. In no-git
/// mode the state branch doesn't exist, so the marker commit has
/// nowhere to land — fail loudly instead of silently succeeding.
#[test]
fn forget_half_push_rejects_no_git_mode() {
    let dir = tmp();
    let tasks_tmp = tmp();
    let tasks_path = tasks_tmp.path().join("tasks");
    bl(dir.path())
        .args(["init", "--tasks-dir", tasks_path.to_str().unwrap()])
        .assert()
        .success();

    let out = bl(dir.path())
        .args(["repair", "--forget-all-half-pushes"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "forget in no-git mode must fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("non-stealth git-backed repo"),
        "expected no-git rejection reason: {stderr}"
    );
}

/// `--forget-all-half-pushes` with nothing to retract is a no-op: it
/// prints a clear message and writes no state-branch commits.
#[test]
fn forget_all_half_pushes_when_none_flagged_is_noop() {
    let repo = new_repo();
    init_in(repo.path());

    let remote = new_bare_remote();
    git(
        repo.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );

    let out = bl(repo.path())
        .args(["repair", "--forget-all-half-pushes"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "forget-all on clean repo must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("No half-push warnings to forget."),
        "expected no-op message: {stdout}"
    );

    let state_wt = repo.path().join(".balls/worktree");
    let log = git(&state_wt, &["log", "--pretty=%s"]);
    assert!(
        !log.contains("state: forget-half-push"),
        "expected no forget-half-push commit: {log}"
    );
}
