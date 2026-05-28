//! bl-f788 — post-delivery machinery follows per-task `target_branch`.
//!
//! bl-d4b0 let `bl review` land a task's squash on the task's own
//! `target_branch`. The machinery that runs *after* the squash —
//! `bl sync`'s push and half-push's tag scan — still assumed one
//! repo-level integration branch, so a per-task delivery was stranded
//! locally and falsely flagged. `bl review` now records the effective
//! branch as a `target=<branch>` marker on the state-branch review
//! subject; sync pushes it, half-push scans it. The marker is omitted
//! when it equals the repo default, so a repo with no per-task
//! overrides is byte-identical (and an old client, which stops parsing
//! at the id/`no-code` token, ignores the trailing marker entirely).

mod common;

use common::*;
use std::fs;
use std::path::Path;

fn sha(repo: &Path, refname: &str) -> String {
    git(repo, &["rev-parse", refname]).trim().to_string()
}

fn subject(repo: &Path, refname: &str) -> String {
    git(repo, &["log", "-1", "--format=%s", refname])
}

/// Subjects of the local state branch, newest first.
fn state_log(repo: &Path) -> String {
    git_state(repo, &["log", "balls/tasks", "--format=%s"])
}

fn create_with_target(repo: &Path, title: &str, branch: &str) -> String {
    let out = bl(repo)
        .args(["create", title, "--target-branch", branch])
        .output()
        .expect("bl create");
    assert!(out.status.success(), "bl create --target-branch failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Claim, drop a file in the worktree, and review under `target`.
fn deliver(repo: &Path, title: &str, target: &str, file: &str) -> String {
    let id = create_with_target(repo, title, target);
    bl(repo).args(["claim", &id]).assert().success();
    let wt = worktree_path(repo, &id);
    fs::write(wt.join(file), "work").unwrap();
    bl(repo)
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();
    id
}

/// Clone of an empty bare remote, repo-level `target_branch=develop`,
/// with `develop` and `release` branched off `main`. Only `main` and
/// `develop` are pre-pushed: `release` has no remote-tracking ref
/// until a sync pushes it (the first delivery's branch is brand new on
/// the forge — the realistic hotfix case).
fn alice_with_remote() -> (Repo, Repo) {
    let code = new_bare_remote();
    let alice = clone_from_remote(code.path(), "alice");
    seed_config(alice.path(), &[("target_branch", "develop")]);
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["branch", "develop"]);
    git(alice.path(), &["branch", "release"]);
    git(alice.path(), &["push", "origin", "main"]);
    git(alice.path(), &["push", "origin", "develop"]);
    (code, alice)
}

/// Acceptance 1+2: a per-task delivery to a non-default branch records
/// `target=release`, `bl sync` pushes `release` (with the tag) to the
/// code remote alongside the repo default and the state branch without
/// touching `main`, and the task is not flagged as a half-push by
/// `bl sync` or `bl repair`.
#[test]
fn per_task_delivery_is_pushed_and_not_half_push() {
    let (code, alice) = alice_with_remote();
    let main_remote_before = sha(code.path(), "main");

    let id = deliver(alice.path(), "hotfix", "release", "fix.txt");
    assert!(
        state_log(alice.path()).contains(&format!("state: review {id} target=release")),
        "review subject must record the per-task target: {}",
        state_log(alice.path())
    );

    bl(alice.path()).arg("sync").assert().success();
    assert_eq!(
        sha(alice.path(), "release"),
        sha(code.path(), "release"),
        "sync must push the per-task target branch to the code remote"
    );
    assert!(
        subject(code.path(), "release").contains(&format!("[{id}]")),
        "pushed release carries the delivery tag"
    );
    assert_eq!(
        sha(alice.path(), "develop"),
        sha(code.path(), "develop"),
        "repo default still pushed alongside"
    );
    assert!(
        git_ok(
            code.path(),
            &["rev-parse", "--verify", "--quiet", "refs/heads/balls/tasks"]
        ),
        "state branch pushed alongside"
    );
    assert_eq!(
        sha(code.path(), "main"),
        main_remote_before,
        "sync must not advance main"
    );

    bl(alice.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();
    let out = bl(alice.path()).arg("sync").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "post-close sync: {stderr}");
    assert!(
        !stderr.contains(&format!("state branch records close for {id}")),
        "per-task delivery must not be flagged as a half-push: {stderr}"
    );
    bl(alice.path())
        .args(["repair", "--forget-all-half-pushes"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No half-push warnings to forget."));
}

/// Covers the `target_already_synced` equal/differ paths and the
/// `seen` dedup: a second delivery to `release` re-pushes (local tip
/// moved past the remote-tracking ref), and a follow-up sync with
/// nothing new skips the now-synced branch.
#[test]
fn second_delivery_to_same_target_repushes_then_skips() {
    let (code, alice) = alice_with_remote();
    let first = deliver(alice.path(), "fix one", "release", "a.txt");
    bl(alice.path()).arg("sync").assert().success();
    bl(alice.path())
        .args(["close", &first, "-m", "ok"])
        .assert()
        .success();

    let second = deliver(alice.path(), "fix two", "release", "b.txt");
    bl(alice.path()).arg("sync").assert().success();
    assert_eq!(
        sha(alice.path(), "release"),
        sha(code.path(), "release"),
        "second delivery to release must reach the code remote"
    );
    assert!(subject(code.path(), "release").contains(&format!("[{second}]")));

    // Nothing new: release is already synced, so the recorded-target
    // push is a no-op and sync still succeeds.
    bl(alice.path()).arg("sync").assert().success();
    assert_eq!(sha(alice.path(), "release"), sha(code.path(), "release"));
}

/// Acceptance 3: with no per-task override the review subject is
/// byte-identical to before bl-f788 — no `target=` marker — both when
/// `target_branch` is unset entirely and when the task's target equals
/// the repo default (the marker is omitted, not just absent).
#[test]
fn no_override_review_subject_is_byte_identical() {
    let plain = new_repo();
    init_in(plain.path());
    let id = create_task(plain.path(), "plain");
    bl(plain.path()).args(["claim", &id]).assert().success();
    fs::write(
        worktree_path(plain.path(), &id).join("f.txt"),
        "x",
    )
    .unwrap();
    bl(plain.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();
    let log = state_log(plain.path());
    assert!(
        log.contains(&format!("state: review {id}")) && !log.contains("target="),
        "unset target_branch must not emit a marker: {log}"
    );

    // Post-XDG (SPEC §6.7) the repo-level `target_branch` field is
    // retired; the resolution chain is `task.target_branch ?? HEAD@root`.
    // So "per-task target equal to the repo default" becomes "per-task
    // target equal to the checked-out branch (HEAD@root)" — the marker
    // must be omitted in that case too.
    let repo = new_repo();
    init_in(repo.path());
    let head_branch = git(repo.path(), &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string();
    let id = deliver(repo.path(), "matches default", &head_branch, "g.txt");
    let log = state_log(repo.path());
    assert!(
        log.contains(&format!("state: review {id}")) && !log.contains("target="),
        "per-task target equal to HEAD@root must omit the marker: {log}"
    );
}

/// The latent repo-level bug bl-f788 also closes: if `target_branch`
/// changes between a task's delivery and a later sync so the recorded
/// target now *equals* the repo default, half-push must not flag it
/// (it scans the — now repo-level — branch and finds the tag) and the
/// recorded-target push must skip it (already pushed as the default).
#[test]
fn recorded_target_that_became_repo_default_is_coherent() {
    let (_code, alice) = alice_with_remote();
    let id = deliver(alice.path(), "hotfix", "release", "fix.txt");
    bl(alice.path()).arg("sync").assert().success();
    bl(alice.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();

    // Re-point the repo default at the branch the task delivered to.
    seed_config(alice.path(), &[("target_branch", "release")]);
    let out = bl(alice.path()).arg("sync").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "sync after retarget: {stderr}");
    assert!(
        !stderr.contains(&format!("state branch records close for {id}")),
        "recorded target that became the repo default must stay coherent: {stderr}"
    );
}

/// A non-fast-forward push of a recorded target branch must not fail
/// the whole sync: it warns and continues, exactly as the repo-level
/// main push is tolerated. The next sync retries (the tip is still
/// ahead of the remote-tracking ref), so the delivery is not lost.
#[test]
fn recorded_target_push_failure_is_best_effort() {
    let code = new_bare_remote();
    let alice = clone_from_remote(code.path(), "alice");
    seed_config(alice.path(), &[("target_branch", "develop")]);
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["branch", "develop"]);
    git(alice.path(), &["branch", "release"]);
    git(alice.path(), &["push", "origin", "main"]);
    git(alice.path(), &["push", "origin", "develop"]);
    git(alice.path(), &["push", "origin", "release"]);

    // A second clone advances origin/release, so alice's later squash
    // on release diverges from the remote and cannot fast-forward.
    let bob = clone_from_remote(code.path(), "bob");
    git(bob.path(), &["checkout", "release"]);
    fs::write(bob.path().join("bob.txt"), "bob").unwrap();
    git(bob.path(), &["add", "-A"]);
    git(bob.path(), &["commit", "-m", "bob advances release"]);
    git(bob.path(), &["push", "origin", "release"]);

    deliver(alice.path(), "hotfix", "release", "fix.txt");
    let out = bl(alice.path()).arg("sync").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "sync must succeed despite a rejected target push: {stderr}"
    );
    assert!(
        stderr.contains("failed to push per-task target branch release"),
        "a rejected recorded-target push must warn, not abort: {stderr}"
    );
    // The squash is committed locally on `release`; only the push was
    // rejected. A retry still sees the branch as not-yet-synced (local
    // tip ahead of the remote-tracking ref) and attempts it again.
    let out = bl(alice.path()).arg("sync").output().unwrap();
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("failed to push per-task target branch release"),
        "an unresolved divergence keeps failing on retry, not silently drops"
    );
}
