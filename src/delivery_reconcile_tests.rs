//! [`Project::reconcile`] — the bl-22dd checkout sync, on throwaway repos where
//! `main` is checked out at the project root (the user's primary checkout).

use super::*;
use crate::delivery::Repo;
use crate::delivery_repo::tests::project;
use std::fs;

/// The end-to-end regression: a delivery must leave the checkout that owns
/// `main` reflecting the change in its working tree — not a phantom staged diff.
#[test]
fn deliver_leaves_the_owning_checkout_clean_not_phantom_staged() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();

    // Index + working tree are at the moved ref — no staged phantom (bl-22dd).
    assert_eq!(Project::run(&root, &["status", "--porcelain"]).unwrap(), "");
    assert_eq!(fs::read_to_string(root.join("feature.txt")).unwrap(), "shipped\n");
    // The ref move carries the delivery subject, not a blank update-ref reflog.
    let reflog = Project::run(&root, &["reflog", "main"]).unwrap();
    assert!(reflog.lines().next().unwrap().contains("Add feature [bl-x]"), "reflog: {reflog}");
}

/// Re-running on an already-synced checkout is a no-op, and a genuine local edit
/// is never clobbered (the gate refuses any checkout that is not pristine one
/// commit behind the ref).
#[test]
fn reconcile_is_idempotent_and_never_clobbers_a_real_edit() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();

    // Already synced: index is at HEAD, not HEAD^, so the gate skips it.
    p.reconcile("main").unwrap();
    assert_eq!(Project::run(&root, &["status", "--porcelain"]).unwrap(), "");

    // A real uncommitted edit survives untouched (worktree != index → gate skip).
    fs::write(root.join("feature.txt"), "local edit\n").unwrap();
    p.reconcile("main").unwrap();
    assert_eq!(fs::read_to_string(root.join("feature.txt")).unwrap(), "local edit\n");
}

/// The dangerous case: a phantom IS present but the checkout also carries a real
/// edit. Reconcile must refuse — leaving both the edit and the phantom for the
/// human, never silently resetting over the edit.
#[test]
fn reconcile_refuses_a_phantom_checkout_carrying_a_real_edit() {
    let (_tmp, root, p) = project();
    let g = |args: &[&str]| Project::run(&root, args).unwrap();
    let c0 = g(&["rev-parse", "HEAD"]).trim().to_string();
    // Hand-build the plumbing-delivery phantom: stage a tree adding feature.txt,
    // record it, then move main to a commit of that tree WITHOUT updating the
    // checkout — index + worktree fall back to the parent.
    fs::write(root.join("feature.txt"), "shipped\n").unwrap();
    g(&["add", "-A"]);
    let tree = g(&["write-tree"]).trim().to_string();
    g(&["reset", "--hard", &c0]);
    let c1 = g(&["commit-tree", &tree, "-p", &c0, "-m", "Add feature [bl-x]"]).trim().to_string();
    g(&["update-ref", "main", &c1]);

    // A genuine edit now sits on top of the phantom.
    fs::write(root.join("seed.txt"), "tampered\n").unwrap();
    p.reconcile("main").unwrap();

    assert_eq!(fs::read_to_string(root.join("seed.txt")).unwrap(), "tampered\n"); // edit kept
    assert!(!root.join("feature.txt").exists()); // checkout left at the parent tree
}

/// `checkouts_on` returns only the checkout(s) that own the branch — never an
/// agent's `work/<id>` worktree (on its own branch) nor the bare root.
#[test]
fn checkouts_on_returns_only_the_branch_owners() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();

    let owners = p.checkouts_on("main").unwrap();
    assert_eq!(owners.len(), 1);
    assert_eq!(owners[0].canonicalize().unwrap(), root.canonicalize().unwrap());

    // The work tree owns its own branch, and is excluded from main's owners.
    let work = p.checkouts_on("work/bl-x").unwrap();
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].canonicalize().unwrap(), wt.canonicalize().unwrap());
}
