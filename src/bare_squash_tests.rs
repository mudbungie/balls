//! Unit tests for the small leaf helpers in `bare_squash`. End-to-end
//! behavior of `squash_into_main` (bare-repo route, in-place case,
//! no-code case) is exercised by `tests/bare_squash.rs`; this file
//! targets the predicates that the squash and rewind paths share.

use super::*;
use crate::config::Config;
use crate::error::BallError;
use crate::git_test_support::{git_run, init_repo};
use tempfile::tempdir;

#[test]
fn is_bare_repo_false_on_regular_repo() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    assert!(!is_bare_repo(td.path()).unwrap());
}

#[test]
fn is_bare_repo_true_when_core_bare_flag_set() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    git_run(td.path(), &["config", "core.bare", "true"]);
    assert!(is_bare_repo(td.path()).unwrap());
}

#[test]
fn is_bare_repo_errors_on_non_git_path() {
    let td = tempdir().unwrap();
    let err = is_bare_repo(td.path()).unwrap_err();
    assert!(matches!(err, BallError::Git(_)), "got {err:?}");
}

#[test]
fn integration_branch_is_checked_out_true_when_checkout_matches() {
    // Non-bare repo, `main` checked out: the resync predicate fires
    // for an integration target of `main`. Any other target name
    // returns false, mirroring the pre-cb73 `squashes_in_place` shape
    // that decided whether to touch the user's work tree.
    let td = tempdir().unwrap();
    init_repo(td.path());
    assert!(integration_branch_is_checked_out(td.path(), "main").unwrap());
    assert!(!integration_branch_is_checked_out(td.path(), "develop").unwrap());
}

#[test]
fn integration_branch_is_checked_out_false_on_bare_repo() {
    // A bare root has no work tree, so the resync predicate is never
    // true regardless of the integration branch name.
    let td = tempdir().unwrap();
    init_repo(td.path());
    git_run(td.path(), &["config", "core.bare", "true"]);
    assert!(!integration_branch_is_checked_out(td.path(), "main").unwrap());
}

#[test]
fn default_config_integration_branch_falls_back_to_checkout() {
    // The `None` arm of the seam: an unset target_branch resolves to
    // the branch checked out at the root — `main` here.
    let td = tempdir().unwrap();
    init_repo(td.path());
    assert_eq!(
        Config::default().integration_branch(td.path()).unwrap(),
        "main"
    );
}
