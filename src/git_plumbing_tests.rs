//! Unit tests for the plumbing helpers. End-to-end coverage of the
//! commit-tree + update-ref pair lives in `tests/bare_squash.rs` (the
//! squash path) and `tests/review_safety.rs` (the rewind path); this
//! file exercises the leaf-level failure modes.

use super::*;
use crate::error::BallError;
use crate::git_test_support::{git_run, init_repo};
use tempfile::tempdir;

#[test]
fn commit_tree_writes_commit_at_existing_tree() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    let tree = crate::git::git_resolve_sha(td.path(), "HEAD^{tree}").unwrap();
    let parent = crate::git::git_resolve_sha(td.path(), "HEAD").unwrap();
    let sha = git_commit_tree(td.path(), &tree, &[&parent], "synthetic squash").unwrap();
    // The synthesized commit is a real object: rev-parse can resolve
    // it and its subject matches the message we passed in.
    let resolved = crate::git::git_resolve_sha(td.path(), &sha).unwrap();
    assert_eq!(resolved, sha);
    assert_eq!(
        crate::git::git_commit_subject(td.path(), &sha).as_deref(),
        Some("synthetic squash"),
    );
}

#[test]
fn commit_tree_errors_on_bogus_tree() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    let err = git_commit_tree(td.path(), "deadbeef", &[], "msg").unwrap_err();
    assert!(matches!(err, BallError::Git(_)), "got {err:?}");
}

#[test]
fn update_ref_moves_branch_to_sha() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    let head = crate::git::git_resolve_sha(td.path(), "HEAD").unwrap();
    // Make a second commit so we have a SHA distinct from HEAD to
    // move the ref to.
    std::fs::write(td.path().join("a"), "1").unwrap();
    git_run(td.path(), &["add", "a"]);
    git_run(td.path(), &["commit", "-m", "second"]);
    let second = crate::git::git_resolve_sha(td.path(), "HEAD").unwrap();
    git_update_ref(td.path(), "refs/heads/scratch", &head).unwrap();
    assert_eq!(
        crate::git::git_resolve_sha(td.path(), "refs/heads/scratch").unwrap(),
        head,
    );
    git_update_ref(td.path(), "refs/heads/scratch", &second).unwrap();
    assert_eq!(
        crate::git::git_resolve_sha(td.path(), "refs/heads/scratch").unwrap(),
        second,
    );
}

#[test]
fn update_ref_errors_on_invalid_ref_name() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    // Names containing `..` are rejected by git's refname validator.
    let err = git_update_ref(td.path(), "refs/heads/bad..name", "deadbeef").unwrap_err();
    assert!(matches!(err, BallError::Git(_)), "got {err:?}");
}
