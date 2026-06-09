//! [`Git`] anvil tests — exercised on throwaway repos so every git act and
//! its failure path is covered without touching the dev repo.

use super::*;
use std::fs;
use tempfile::TempDir;

/// A throwaway repo: a checkout with one commit on the anvil.
/// Returns the tempdir (kept alive), the checkout path, and a [`Git`].
fn repo() -> (TempDir, PathBuf, Git) {
    let tmp = TempDir::new().unwrap();
    let checkout = tmp.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    let git = |args: &[&str]| run(&checkout, args, None).unwrap();
    git(&["init", "-q"]);
    git(&["config", "user.name", "test"]);
    git(&["config", "user.email", "test@example.com"]);
    fs::write(checkout.join("seed.txt"), "seed\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "seed"]);
    let g = Git::at(&checkout);
    (tmp, checkout, g)
}

#[test]
fn head_returns_the_anvil_tip() {
    let (_tmp, _op, g) = repo();
    let head = g.head().unwrap();
    assert_eq!(head.len(), 40); // a full sha
}

#[test]
fn open_seal_advances_the_anvil_to_the_committed_change() {
    let (tmp, checkout, g) = repo();
    let before = g.head().unwrap();
    let change = tmp.path().join("change");

    g.open(&change).unwrap();
    fs::write(change.join("tasks.md"), "a staged change\n").unwrap();
    let sealed = g.seal(&change, "Author a task\n\nbl-op: create\n").unwrap();

    // The anvil fast-forwarded onto the sealed commit, and checkout's tree
    // now carries the change.
    assert_eq!(g.head().unwrap(), sealed);
    assert_ne!(sealed, before);
    assert_eq!(fs::read_to_string(checkout.join("tasks.md")).unwrap(), "a staged change\n");
}

#[test]
fn unseal_resets_the_anvil_back_to_a_prior_tip() {
    let (tmp, checkout, g) = repo();
    let before = g.head().unwrap();
    let change = tmp.path().join("change");

    g.open(&change).unwrap();
    fs::write(change.join("tasks.md"), "x\n").unwrap();
    g.seal(&change, "land it\n").unwrap();
    assert_ne!(g.head().unwrap(), before);

    g.unseal(&before).unwrap();
    assert_eq!(g.head().unwrap(), before);
    assert!(!checkout.join("tasks.md").exists()); // the change is gone again
}

#[test]
fn sealing_an_unchanged_tree_converges_on_the_existing_tip() {
    // The no-op seal: identical content stages nothing, so no empty commit
    // lands and the anvil tip stands (§13 idempotence — re-install).
    let (tmp, _checkout, g) = repo();
    let before = g.head().unwrap();
    let change = tmp.path().join("change");
    g.open(&change).unwrap();
    let sealed = g.seal(&change, "would-be empty\n").unwrap();
    assert_eq!(sealed, before);
}

#[test]
fn close_removes_the_change_worktree() {
    let (tmp, _op, g) = repo();
    let change = tmp.path().join("change");
    g.open(&change).unwrap();
    assert!(change.is_dir());
    g.close(&change).unwrap();
    assert!(!change.exists());
}

#[test]
fn head_on_a_non_repo_is_an_error() {
    let tmp = TempDir::new().unwrap();
    let err = Git::at(&tmp.path().join("nope")).head().unwrap_err();
    assert!(err.to_string().contains("git rev-parse HEAD"));
}

#[test]
fn opening_the_same_change_worktree_twice_is_an_error() {
    let (tmp, _op, g) = repo();
    let change = tmp.path().join("change");
    g.open(&change).unwrap();
    assert!(g.open(&change).is_err());
}

#[test]
fn seal_fails_when_the_anvil_cannot_fast_forward() {
    let (tmp, checkout, g) = repo();
    let change = tmp.path().join("change");
    g.open(&change).unwrap();

    // Advance the anvil independently so the change no longer fast-forwards.
    fs::write(checkout.join("other.txt"), "diverge\n").unwrap();
    run(&checkout, &["add", "-A"], None).unwrap();
    run(&checkout, &["commit", "-q", "-m", "diverge"], None).unwrap();

    fs::write(change.join("tasks.md"), "y\n").unwrap();
    let err = g.seal(&change, "wont ff\n").unwrap_err();
    assert!(err.to_string().contains("git merge --ff-only"));
}

#[test]
fn a_lost_seal_resets_the_checkout_so_later_ops_succeed() {
    let (tmp, checkout, g) = repo();
    let change = tmp.path().join("change");
    g.open(&change).unwrap();
    fs::write(change.join("seed.txt"), "claimant = \"winner\"\n").unwrap();

    // The bl-07d6 wedge state: a claimant write left modified AND STAGED in
    // the checkout, against the very file the seal's merge must update — the
    // ff-only merge aborts ("Your local changes ... would be overwritten").
    fs::write(checkout.join("seed.txt"), "claimant = \"phantom\"\n").unwrap();
    run(&checkout, &["add", "-A"], None).unwrap();

    let err = g.seal(&change, "loses the race\n").unwrap_err();
    assert!(err.to_string().contains("git merge --ff-only"));

    // The failed seal rolled the checkout back atomically: clean tree, no
    // staged phantom claimant left to wedge or mislead later reads.
    assert_eq!(run(&checkout, &["status", "--porcelain"], None).unwrap(), "");
    assert_eq!(fs::read_to_string(checkout.join("seed.txt")).unwrap(), "seed\n");

    // ...and the clone is not wedged: the next op seals cleanly.
    let retry = tmp.path().join("retry");
    g.open(&retry).unwrap();
    fs::write(retry.join("seed.txt"), "claimant = \"winner\"\n").unwrap();
    let sealed = g.seal(&retry, "wins cleanly\n").unwrap();
    assert_eq!(g.head().unwrap(), sealed);
}

#[test]
fn unseal_to_an_unknown_sha_is_an_error() {
    let (_tmp, _op, g) = repo();
    assert!(g.unseal("0000000000000000000000000000000000000000").is_err());
}

#[test]
fn closing_a_path_that_is_not_a_worktree_is_an_error() {
    let (tmp, _op, g) = repo();
    assert!(g.close(&tmp.path().join("never-opened")).is_err());
}
