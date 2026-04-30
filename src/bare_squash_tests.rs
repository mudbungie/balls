//! Unit tests for bare_squash helpers. The end-to-end behavior of
//! `squash_into_main` (including the bare-repo route) is exercised by
//! `tests/bare_squash.rs`; this file targets the small leaf helpers
//! and their failure paths so coverage stays at 100%.

use super::*;
use crate::error::BallError;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

/// Scrub git environment variables that a parent process (e.g. a
/// pre-commit hook running these tests) may have leaked. Without
/// this, a freshly-spawned `git init` inside a tempdir resolves to
/// the parent repo's gitdir and the test bombs.
fn raw_git(path: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new("git");
    cmd.current_dir(path).args(args);
    for var in crate::git::GIT_ENV_VARS {
        cmd.env_remove(var);
    }
    cmd.output().expect("spawn git")
}

/// Initialize a fresh non-bare repo at `path` with a configured user
/// and an initial commit on `main`. Errors short-circuit the test
/// downstream rather than producing a custom assertion message — the
/// extra format args otherwise show up as uncovered "panic-branch"
/// lines in tarpaulin's line coverage.
fn init_repo(path: &Path) {
    let run = |args: &[&str]| {
        assert!(raw_git(path, args).status.success());
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "test"]);
    run(&["config", "commit.gpgsign", "false"]);
    run(&["commit", "--allow-empty", "-m", "init"]);
}

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
    assert!(raw_git(td.path(), &["config", "core.bare", "true"])
        .status
        .success());
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
    assert!(raw_git(
        td.path(),
        &["worktree", "add", "--detach", &wt_str, "HEAD"]
    )
    .status
    .success());
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
