//! [`Project::prune`] tests — the §11/§14 deferred `work/<id>` branch cleanup
//! on throwaway project repos: delete the settled (delivered / forkless),
//! preserve the undelivered, refuse nothing a live worktree holds.

use std::fs;

use crate::delivery::Repo;
use crate::delivery_repo::tests::{project, tip};
use crate::delivery_repo::Project;

#[test]
fn prune_deletes_a_delivered_branch_after_teardown_and_reruns_clean() {
    // The leak this fixes (bl-292d): close delivered + released the worktree
    // but the branch stayed — prune is the deferred cleanup that removes it.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    p.release(&wt).unwrap(); // close.post teardown: worktree gone, branch left
    // Integration moves on, so the branch tree differs from main's tip — only
    // the fork-scoped [bl-x] delivery scan can call it settled.
    fs::write(root.join("other.txt"), "other\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "concurrent work"]).unwrap();

    p.prune().unwrap();
    assert!(!Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-x"]).unwrap());
    assert_eq!(tip(&root), "concurrent work"); // prune touches no integration commit

    p.prune().unwrap(); // idempotent: the pruned branch no longer enumerates
}

#[test]
fn prune_deletes_a_forkless_branch_the_unclaim_of_uncommitted_work_left() {
    // unclaim released the worktree (uncommitted edits discarded with it); the
    // branch carries no commit beyond its fork — deleting it loses nothing.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-y").unwrap();
    p.release(&wt).unwrap(); // unclaim.post

    p.prune().unwrap();
    assert!(!Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-y"]).unwrap());
}

#[test]
fn prune_preserves_a_branch_carrying_undelivered_commits() {
    // The bl-65e0 unclaim contract: work COMMITTED on work/<id> survives — a
    // later claim reattaches the branch and a later close DELIVERS it.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-z").unwrap();
    fs::write(wt.join("kept.txt"), "undelivered\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    p.release(&wt).unwrap(); // unclaim.post: worktree gone, committed work only on the branch

    p.prune().unwrap();
    assert!(Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-z"]).unwrap());

    // And the survival pays off: reclaim + close delivers the preserved work.
    p.materialize(&wt, "work/bl-z").unwrap();
    p.deliver(&wt, "work/bl-z", "main", "Land it [bl-z]", "[bl-z]").unwrap();
    assert_eq!(tip(&root), "Land it [bl-z]");
}

#[test]
fn prune_leaves_a_checked_out_branch_and_still_succeeds() {
    // A live claim's branch is settled (fresh fork, no commits) but checked out
    // in its worktree — git refuses the delete, and prune, being best-effort,
    // keeps going instead of failing the prime.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-live").unwrap();

    p.prune().unwrap();
    assert!(Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-live"]).unwrap());
    assert!(wt.join("seed.txt").exists()); // the worktree is untouched
}

#[test]
fn prune_on_a_root_that_is_no_repo_is_a_quiet_no_op() {
    // A pre-claim prime in a project with no git repo yet: nothing to prune,
    // nothing to fail (the prune is best-effort end to end).
    let outside = tempfile::TempDir::new().unwrap();
    Project::at(outside.path()).prune().unwrap();
}

#[test]
fn prune_scopes_the_delivery_scan_to_the_fork_so_a_reused_id_survives() {
    // A prior incarnation's [bl-r] delivery is an ancestor of the new branch's
    // fork point (§11 recency) — it must not call the NEW incarnation's
    // undelivered work settled.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-r").unwrap();
    fs::write(wt.join("a.txt"), "1\n").unwrap();
    p.deliver(&wt, "work/bl-r", "main", "first [bl-r]", "[bl-r]").unwrap();
    p.discard(&wt, "work/bl-r").unwrap(); // first incarnation fully torn down

    // Second incarnation forks AFTER the first delivery and commits new work.
    p.materialize(&wt, "work/bl-r").unwrap();
    fs::write(wt.join("b.txt"), "2\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "second wip"]).unwrap();
    p.release(&wt).unwrap();

    p.prune().unwrap();
    assert!(Project::ok(&root, &["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-r"]).unwrap());
}
