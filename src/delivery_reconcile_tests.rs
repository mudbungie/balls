//! [`Project::reconcile`] — the bl-22dd checkout sync, on throwaway repos where
//! `main` is checked out at the project root (the user's primary checkout).

use super::*;
use crate::delivery::Repo;
use crate::delivery_repo::tests::project;
use std::fs;
use std::path::{Path, PathBuf};

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

/// Re-running on an already-synced checkout is a no-op, and a local edit on a
/// checkout already at the tip's tree is left exactly as it is.
#[test]
fn reconcile_is_idempotent_and_never_clobbers_a_real_edit() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();

    p.reconcile("main").unwrap();
    assert_eq!(Project::run(&root, &["status", "--porcelain"]).unwrap(), "");

    fs::write(root.join("feature.txt"), "local edit\n").unwrap();
    assert_eq!(Project::carry_forward(&root, "main").unwrap(), None);
    assert_eq!(fs::read_to_string(root.join("feature.txt")).unwrap(), "local edit\n");
}

/// The close, landed the way the delivery lands it: from a side worktree change
/// `seed.txt` and add `c.txt` (`b.txt` stays untouched), then move `main` by
/// plumbing — the checkout on `main` is never told. `root` must already carry a
/// committed `b.txt` (see [`two_files`]).
fn plumb_delivery(root: &Path) {
    let g = |args: &[&str]| Project::run(root, args).unwrap();
    let side = root.parent().unwrap().join("side");
    g(&["worktree", "add", "-q", "--detach", side.to_str().unwrap(), "main"]);
    fs::write(side.join("seed.txt"), "two\n").unwrap();
    fs::write(side.join("c.txt"), "new\n").unwrap();
    commit_and_move(root, &side);
}

/// Commit whatever `side` holds and move `main` onto it by plumbing.
fn commit_and_move(root: &Path, side: &Path) {
    let g = |args: &[&str]| Project::run(root, args).unwrap();
    Project::run(side, &["add", "-A"]).unwrap();
    Project::run(side, &["commit", "-q", "-m", "delivery"]).unwrap();
    let new = Project::run(side, &["rev-parse", "HEAD"]).unwrap();
    g(&["update-ref", "-m", "delivery", "refs/heads/main", new.trim()]);
}

/// The fixture repo plus a committed `b.txt`, so a delivery has a file to leave alone.
fn two_files() -> (tempfile::TempDir, PathBuf, Project) {
    let (tmp, root, p) = project();
    fs::write(root.join("b.txt"), "one\n").unwrap();
    Project::run(&root, &["add", "b.txt"]).unwrap();
    Project::run(&root, &["commit", "-q", "-m", "b"]).unwrap();
    (tmp, root, p)
}

fn read(root: &Path, f: &str) -> String {
    fs::read_to_string(root.join(f)).unwrap()
}

/// A fresh close is carried forward in full — changed file updated, added file
/// present — with no phantom staged revert and nothing said.
#[test]
fn a_fresh_close_is_carried_forward_including_an_added_file() {
    let (_tmp, root, _p) = two_files();
    plumb_delivery(&root);
    assert_eq!(Project::carry_forward(&root, "main").unwrap(), None);
    assert_eq!(read(&root, "seed.txt"), "two\n");
    assert_eq!(read(&root, "c.txt"), "new\n");
    assert_eq!(Project::run(&root, &["status", "--porcelain"]).unwrap(), "");
}

/// The bl-b69e widening: a local edit the close did not touch no longer blocks
/// the carry — it rides forward and is the only remaining difference.
#[test]
fn an_unrelated_local_edit_is_carried_forward() {
    let (_tmp, root, p) = two_files();
    plumb_delivery(&root);
    fs::write(root.join("b.txt"), "mine\n").unwrap();
    p.reconcile("main").unwrap();
    assert_eq!(read(&root, "seed.txt"), "two\n");
    assert_eq!(read(&root, "b.txt"), "mine\n");
    assert_eq!(Project::run(&root, &["status", "--porcelain"]).unwrap(), " M b.txt\n");
}

/// Two closes land before any sync: `<old>` is found by the index's tree in the
/// reflog, not assumed to be `HEAD^`, so both deliveries arrive.
#[test]
fn two_unsynced_closes_are_carried_forward_together() {
    let (_tmp, root, _p) = two_files();
    plumb_delivery(&root);
    let side = root.parent().unwrap().join("side");
    fs::write(side.join("d.txt"), "later\n").unwrap();
    commit_and_move(&root, &side);
    assert_eq!(Project::carry_forward(&root, "main").unwrap(), None);
    assert_eq!(read(&root, "c.txt"), "new\n");
    assert_eq!(read(&root, "d.txt"), "later\n");
    assert_eq!(Project::run(&root, &["status", "--porcelain"]).unwrap(), "");
}

/// A delivered file also edited locally: git refuses, nothing is touched or
/// half-applied, and the warning names the exact command to run.
#[test]
fn a_touched_file_edited_locally_is_left_alone_with_the_command() {
    let (_tmp, root, p) = two_files();
    plumb_delivery(&root);
    fs::write(root.join("seed.txt"), "mine\n").unwrap();
    p.reconcile("main").unwrap(); // prints the warning, fails nothing
    let warning = Project::carry_forward(&root, "main").unwrap().unwrap();
    assert_eq!(read(&root, "seed.txt"), "mine\n");
    assert!(!root.join("c.txt").exists());
    assert!(warning.contains("NOT carried forward"), "{warning}");
    assert!(warning.contains("read-tree -m -u"), "{warning}");
}

/// Staged edits: the index tree matches no recorded `main` commit, so no `<old>`
/// can be named safely — the staged edit is kept, nothing moves, and it says why.
#[test]
fn a_staged_index_is_left_alone_and_says_why() {
    let (_tmp, root, _p) = two_files();
    plumb_delivery(&root);
    fs::write(root.join("b.txt"), "staged\n").unwrap();
    Project::run(&root, &["add", "b.txt"]).unwrap();
    let warning = Project::carry_forward(&root, "main").unwrap().unwrap();
    assert!(warning.contains("left alone"), "{warning}");
    assert_eq!(Project::run(&root, &["show", ":b.txt"]).unwrap(), "staged\n");
    assert!(!root.join("c.txt").exists());
}

/// A conflict in progress (unmerged index entries) is the human's: skipped
/// silently, index untouched.
#[test]
fn an_unmerged_index_is_skipped() {
    let (_tmp, root, _p) = two_files();
    let g = |args: &[&str]| Project::run(&root, args).unwrap();
    g(&["checkout", "-q", "-b", "other"]);
    fs::write(root.join("b.txt"), "theirs\n").unwrap();
    g(&["commit", "-q", "-am", "theirs"]);
    g(&["checkout", "-q", "main"]);
    fs::write(root.join("b.txt"), "ours\n").unwrap();
    g(&["commit", "-q", "-am", "ours"]);
    assert!(!Project::ok(&root, &["merge", "other"]).unwrap());
    plumb_delivery(&root);
    assert_eq!(Project::carry_forward(&root, "main").unwrap(), None);
    assert!(!root.join("c.txt").exists());
}

/// A checkout not on `main` — another branch, or detached — is not the
/// integration checkout: reconcile never touches it.
#[test]
fn a_checkout_on_another_branch_or_detached_is_untouched() {
    for how in [&["checkout", "-q", "-b", "feature"][..], &["checkout", "-q", "--detach"][..]] {
        let (_tmp, root, p) = two_files();
        Project::run(&root, how).unwrap();
        plumb_delivery(&root);
        p.reconcile("main").unwrap();
        assert_eq!(read(&root, "seed.txt"), "seed\n");
        assert!(!root.join("c.txt").exists());
    }
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
