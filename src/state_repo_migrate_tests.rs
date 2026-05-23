//! Tests for the legacy-worktree migration guard (bl-c7b5).

use crate::state_repo::ensure;
use crate::state_repo::test_support::{implicit, legacy_project};
use std::fs;

#[test]
fn ensure_refuses_migration_with_uncommitted_legacy_worktree_state() {
    // The migration adopts only the committed tip of balls/tasks via
    // fetch+reset; uncommitted edits and untracked files in the legacy
    // .balls/worktree would be silently discarded. Refuse instead.
    let project = legacy_project();
    let root = project.path();
    let wt = root.join(".balls/worktree");
    fs::write(wt.join(".balls/tasks/bl-legacytask.json"), "{\"dirty\":true}\n").unwrap();
    fs::write(wt.join(".balls/tasks/bl-untracked.json"), "{}\n").unwrap();

    let err = ensure(root, &implicit()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("uncommitted"), "names the situation: {msg}");
    assert!(msg.contains("bl-legacytask.json"), "lists the tracked edit: {msg}");
    assert!(msg.contains("bl-untracked.json"), "lists the untracked file: {msg}");
    assert!(msg.contains("git add -A && git commit"), "shows the recovery command: {msg}");

    // Recoverable: legacy worktree intact, no partial state-repo materialized.
    assert!(wt.exists());
    assert!(!root.join(".balls/state-repo").exists());
    // A retry after committing the changes succeeds cleanly.
    crate::git_test_support::git_run(&wt, &["add", "-A"]);
    crate::git_test_support::git_run(&wt, &["commit", "-qm", "preserve", "--no-verify"]);
    let dir = ensure(root, &implicit()).unwrap();
    assert!(dir.join(".balls/tasks/bl-legacytask.json").exists());
    assert!(dir.join(".balls/tasks/bl-untracked.json").exists());
}
