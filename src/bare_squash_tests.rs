//! Unit tests for bare_squash helpers. The end-to-end behavior of
//! `squash_into_main` (including the bare-repo route) is exercised by
//! `tests/bare_squash.rs`; this file targets the small leaf helpers
//! and their failure paths so coverage stays at 100%.

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
fn worktree_add_detach_errors_on_bogus_ref() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    let target = td.path().join("wt");
    let err = worktree_add_detach(td.path(), &target, "no-such-ref-anywhere").unwrap_err();
    assert!(matches!(err, BallError::Git(_)), "got {err:?}");
    assert!(!target.exists(), "no worktree should have been created");
}

#[test]
fn update_ref_errors_on_invalid_ref_name() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    // Names containing `..` are rejected by git's refname validator.
    let err =
        update_ref(td.path(), "refs/heads/bad..name", "deadbeef").unwrap_err();
    assert!(matches!(err, BallError::Git(_)), "got {err:?}");
}

#[test]
fn scrub_path_noop_when_path_missing() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    let absent = td.path().join("does-not-exist");
    scrub_path(td.path(), &absent);
    assert!(!absent.exists());
}

#[test]
fn scrub_path_removes_orphaned_directory() {
    // A leftover directory that isn't a registered worktree: the
    // `git worktree remove` call fails silently, then the directory
    // is removed by the fs path. Both branches of the function fire.
    let td = tempdir().unwrap();
    init_repo(td.path());
    let leftover = td.path().join("stale");
    std::fs::create_dir(&leftover).unwrap();
    std::fs::write(leftover.join("trash"), "junk").unwrap();
    scrub_path(td.path(), &leftover);
    assert!(!leftover.exists(), "scrub_path should remove orphaned dir");
}

#[test]
fn scrub_path_removes_registered_worktree() {
    // The happy path through scrub: `git worktree remove` succeeds
    // and clears the directory in one shot, so the second `if
    // path.exists()` is skipped.
    let td = tempdir().unwrap();
    init_repo(td.path());
    let wt = td.path().join("live-wt");
    let wt_str = wt.to_string_lossy().to_string();
    git_run(td.path(), &["worktree", "add", "--detach", &wt_str, "HEAD"]);
    scrub_path(td.path(), &wt);
    assert!(!wt.exists(), "scrub_path should remove registered worktree");
}

#[test]
fn squash_worktree_path_includes_pid() {
    // Stable property: the temp path lives under the store's local
    // dir and is namespaced by pid so concurrent processes don't
    // collide. Use Store::init to avoid touching its private fields.
    let td = tempdir().unwrap();
    init_repo(td.path());
    let store = Store::init(td.path(), false, None).unwrap();
    let p = squash_worktree_path(&store);
    assert!(p.starts_with(store.local_dir()));
    let name = p.file_name().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with("squash-"));
    assert!(name.ends_with(&std::process::id().to_string()));
}

#[test]
fn squashes_in_place_true_only_when_checkout_is_target() {
    // Non-bare repo, `main` checked out: an integration target of
    // `main` squashes in place (today's byte-identical default); any
    // other target must route through the detached worktree so the
    // user's checkout is never disturbed.
    let td = tempdir().unwrap();
    init_repo(td.path());
    let store = Store::init(td.path(), false, None).unwrap();
    assert!(squashes_in_place(&store, "main").unwrap());
    assert!(!squashes_in_place(&store, "develop").unwrap());
}

#[test]
fn squashes_in_place_false_on_bare_repo() {
    // A bare root has no work tree, so the in-place squash is never
    // valid regardless of the integration branch name.
    let td = tempdir().unwrap();
    init_repo(td.path());
    let store = Store::init(td.path(), false, None).unwrap();
    git_run(td.path(), &["config", "core.bare", "true"]);
    assert!(!squashes_in_place(&store, "main").unwrap());
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
