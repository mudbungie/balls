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
    let git = |args: &[&str]| Git::run(&checkout, args, None).unwrap();
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
    Git::run(&checkout, &["add", "-A"], None).unwrap();
    Git::run(&checkout, &["commit", "-q", "-m", "diverge"], None).unwrap();

    fs::write(change.join("tasks.md"), "y\n").unwrap();
    let err = g.seal(&change, "wont ff\n").unwrap_err();
    assert!(err.to_string().contains("git merge --ff-only"));
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
