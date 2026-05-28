//! Review's squash/delivery edge cases, split from `review.rs`: the
//! empty-worktree no-op, the `--no-worktree` metadata-only flip, and
//! the squash commit subject. The submit→reviewer→close flow stays
//! in `review.rs`.

mod common;

use common::*;

#[test]
fn review_of_empty_worktree_leaves_delivered_in_null() {
    // No-op review: agent claims, edits nothing, reviews. No commit
    // should land on main, delivered_in must be null (not the current
    // HEAD), and the state-branch subject carries `no-code`.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "gate check");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();

    let head_before = git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();
    bl(repo.path())
        .args(["review", &id, "-m", "nothing to deliver"])
        .assert()
        .success();
    let head_after = git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();

    assert_eq!(head_before, head_after, "empty squash must not commit on main");
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "review");
    assert!(j["delivered_in"].is_null(), "delivered_in must be null: {j}");

    // State branch subject ends with "no-code".
    let state_log = git_state(
        repo.path(),
        &["log", "--format=%s", "balls/tasks"],
    );
    let line = state_log
        .lines()
        .find(|l| l.starts_with(&format!("state: review {id}")))
        .expect("review subject missing");
    assert!(line.ends_with(" no-code"), "expected no-code marker: {line}");
}

#[test]
fn review_no_worktree_claim_flips_status_without_squash() {
    // A task claimed `--no-worktree` in a git repo has no work branch
    // and no `.balls-worktrees/<id>`. `bl review` must do a metadata-only
    // flip (like no-git mode), not route into the worktree squash path —
    // which previously spawned git in the missing worktree dir and failed
    // with `failed to spawn git: No such file or directory` (bl-7152).
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "archival ball");
    bl_as(repo.path(), "alice")
        .args(["claim", &id, "--no-worktree"])
        .assert()
        .success();

    let head_before = git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();
    bl(repo.path())
        .args(["review", &id, "-m", "nothing to deliver"])
        .assert()
        .success();
    let head_after = git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();

    assert_eq!(head_before, head_after, "no-worktree review must not touch main");
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "review");

    // The rest of the lifecycle still completes: close archives the task
    // and no-ops the absent worktree.
    bl(repo.path())
        .args(["close", &id, "-m", "archived"])
        .assert()
        .success();
    assert!(!discover_tasks_dir(repo.path()).join(format!("{id}.json")).exists());
}

#[test]
fn review_resyncs_working_tree_at_non_bare_root() {
    // bl-cb73 plumbing path: `commit-tree` + `update-ref` moves the
    // integration branch without writing through the work tree. If the
    // root has `main` checked out, the post-review work tree would be
    // stale against HEAD — the squash code must follow up with a
    // `reset --hard HEAD` to re-sync. `git status --porcelain` clean
    // at the root catches the regression where the resync is skipped.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);
    std::fs::write(wt.join("delivered.txt"), "payload").unwrap();

    bl(repo.path())
        .args(["review", &id])
        .assert()
        .success();

    // Squash landed on main.
    assert!(
        repo.path().join("delivered.txt").exists(),
        "resync should materialize the squashed file at the root",
    );
    let status = git(repo.path(), &["status", "--porcelain"]);
    assert!(
        status.trim().is_empty(),
        "root work tree must be clean after review (got: {status:?})",
    );
}

#[test]
fn review_creates_squash_commit_with_title() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);
    std::fs::write(wt.join("work.txt"), "code").unwrap();

    bl(repo.path())
        .args(["review", &id])
        .assert()
        .success();

    // The squash commit should include the task title and id
    let log = git(repo.path(), &["log", "--oneline", "-1"]);
    assert!(log.contains("feature"), "squash commit should contain task title, got: {log}");
    assert!(log.contains(&id), "squash commit should contain task id, got: {log}");
}
