//! bl-6969: the legacy `.balls/local/pending-sync/` migration warning.
//! After the human-gate staging surface was removed, a clone that still
//! carries staged reports from a prior `bl sync --review` gets a
//! one-line warning every `bl` invocation until the operator cleans up.

mod common;

use common::*;
use std::fs;

#[test]
fn pending_sync_directory_with_files_triggers_warning() {
    let repo = new_repo();
    init_in(repo.path());
    let staged = repo.path().join(".balls/local/pending-sync/sync");
    fs::create_dir_all(&staged).unwrap();
    fs::write(staged.join("abcd.json"), b"{}").unwrap();

    let out = bl(repo.path()).arg("list").output().unwrap();
    assert!(out.status.success(), "bl list must still succeed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("staging feature is deferred"),
        "expected deferral warning in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("see bl-6969"),
        "warning must cite bl-6969 so the operator can find context: {stderr}"
    );
    assert!(
        staged.join("abcd.json").exists(),
        "warning must not delete the staged report"
    );
}

#[test]
fn empty_pending_sync_directory_does_not_warn() {
    let repo = new_repo();
    init_in(repo.path());
    fs::create_dir_all(repo.path().join(".balls/local/pending-sync/sync")).unwrap();

    let out = bl(repo.path()).arg("list").output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("staging feature is deferred"),
        "no warning when nothing is staged: {stderr}"
    );
}

#[test]
fn no_pending_sync_directory_does_not_warn() {
    let repo = new_repo();
    init_in(repo.path());

    let out = bl(repo.path()).arg("list").output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("staging feature is deferred"),
        "no warning when the legacy dir was never created: {stderr}"
    );
}
