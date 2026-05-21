//! Unit tests for `run_pre_check` (bl-1f38) — the `review.pre_check`
//! gate's outcomes exercised directly, no git repo needed: unset is a
//! no-op, a zero exit passes, a non-zero exit aborts with a fix-it
//! message, and the command runs with the worktree as its CWD.

use super::run_pre_check;
use tempfile::TempDir;

#[test]
fn unset_pre_check_is_a_no_op() {
    let dir = TempDir::new().unwrap();
    run_pre_check(None, dir.path()).unwrap();
}

#[test]
fn zero_exit_passes() {
    let dir = TempDir::new().unwrap();
    run_pre_check(Some("true"), dir.path()).unwrap();
}

#[test]
fn non_zero_exit_aborts_with_fix_it_message() {
    let dir = TempDir::new().unwrap();
    let err = run_pre_check(Some("exit 7"), dir.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("review pre-check failed"), "got: {msg}");
    assert!(msg.contains("retry `bl review`"), "got: {msg}");
}

#[test]
fn runs_with_the_worktree_as_cwd() {
    // A relative-path probe resolves against `dir`, so the gate sees
    // the worktree's contents — the end-state being delivered.
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("marker"), "x").unwrap();
    run_pre_check(Some("test -f marker"), dir.path()).unwrap();
    assert!(run_pre_check(Some("test -f absent"), dir.path()).is_err());
}
