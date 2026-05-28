//! bl-56f4: review must succeed when `store.root` is a bare gitdir.
//!
//! These tests mimic the lernie layout: a repo whose `.git/` config
//! has `core.bare = true`, with linked worktrees under
//! `.balls-worktrees/<id>/`. Without the fix, `bl review` chdirs into
//! the bare gitdir to run `git merge --squash` and git refuses with
//! "this operation must be run in a work tree".

mod common;

use common::*;

#[test]
fn review_succeeds_when_repo_root_is_bare() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "feature");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);
    std::fs::write(wt.join("feature.txt"), "delivered").unwrap();

    set_core_bare(repo.path());

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

    // Pre-bl-cb73 the squash path provisioned an ephemeral worktree
    // under `<root>/.balls/local/squash-<pid>/` and this assertion
    // guarded against leftovers. bl-cb73 swapped that mechanism for
    // a pure `commit-tree` + `update-ref` plumbing path (see
    // `src/bare_squash.rs:9-16`), so there is no ephemeral worktree
    // anywhere and nothing to leak. The check is retired.
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

    set_core_bare(repo.path());

    let head_before = git(repo.path(), &["rev-parse", "main"]).trim().to_string();
    let wt = worktree_path(repo.path(), &id);
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

    let state_log = git_state(repo.path(), &["log", "--format=%s", "balls/tasks"]);
    let line = state_log
        .lines()
        .find(|l| l.starts_with(&format!("state: review {id}")))
        .expect("review subject missing");
    assert!(line.ends_with(" no-code"), "expected no-code marker: {line}");
}
