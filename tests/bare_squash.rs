//! bl-56f4: review must succeed when `store.root` is a bare gitdir.
//!
//! These tests mimic the lernie layout: a repo whose `.git/` config
//! has `core.bare = true`, with linked worktrees under
//! `.balls-worktrees/<id>/`. Without the fix, `bl review` chdirs into
//! the bare gitdir to run `git merge --squash` and git refuses with
//! "this operation must be run in a work tree".

mod common;

use common::*;

/// Toggle the main repo's `core.bare` flag without going through the
/// `bl` discovery path (which already trusts the gitdir). Mimics the
/// state of the user's bare-flagged main gitdir post-conversion.
fn set_core_bare(repo_root: &std::path::Path, bare: bool) {
    git(repo_root, &["config", "core.bare", if bare { "true" } else { "false" }]);
}

#[test]
fn review_succeeds_when_repo_root_is_bare() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("feature.txt"), "delivered").unwrap();

    set_core_bare(repo.path(), true);

    bl(&wt)
        .args(["review", &id, "-m", "ready for review"])
        .assert()
        .success();

    // The squash landed on main even though the bare main gitdir
    // can't host a working tree.
    let log = git(repo.path(), &["log", "--oneline", "main", "-1"]);
    assert!(
        log.contains(&id),
        "main should carry the squash for {id}: {log}"
    );

    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "review");
    assert!(
        j["delivered_in"].is_string(),
        "delivered_in must be set: {j}"
    );

    // The ephemeral squash worktree must be cleaned up — a leftover
    // would block the next review at `git worktree add`.
    let local = repo.path().join(".balls/local");
    let leaks: Vec<_> = std::fs::read_dir(&local)
        .unwrap()
        .filter_map(|e| {
            let n = e.ok()?.file_name().to_string_lossy().to_string();
            n.starts_with("squash-").then_some(n)
        })
        .collect();
    assert!(leaks.is_empty(), "stale squash worktrees: {leaks:?}");
}

#[test]
fn review_no_code_on_bare_repo_records_no_code_marker() {
    // A no-op review on a bare repo must take the same `delivered_in
    // = null` path as on a non-bare repo. Confirms that the bare
    // detached-worktree route handles empty squashes without leaking
    // a stray commit.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "gate check");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();

    set_core_bare(repo.path(), true);

    let head_before = git(repo.path(), &["rev-parse", "main"]).trim().to_string();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    bl(&wt)
        .args(["review", &id, "-m", "nothing to deliver"])
        .assert()
        .success();
    let head_after = git(repo.path(), &["rev-parse", "main"]).trim().to_string();
    assert_eq!(
        head_before, head_after,
        "empty bare-repo squash must not commit on main"
    );

    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "review");
    assert!(j["delivered_in"].is_null(), "delivered_in must be null: {j}");

    let state_log = git(repo.path(), &["log", "--format=%s", "balls/tasks"]);
    let line = state_log
        .lines()
        .find(|l| l.starts_with(&format!("state: review {id}")))
        .expect("review subject missing");
    assert!(line.ends_with(" no-code"), "expected no-code marker: {line}");
}
