//! Tests for the legacy-worktree migration guard (bl-c7b5).

use crate::state_repo::ensure;
use crate::state_repo::test_support::{implicit, legacy_project};
use crate::{git_state, git_test_support::git_run};
use std::fs;
use tempfile::TempDir;

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

#[test]
fn guard_allows_adoption_when_legacy_worktree_dir_is_absent() {
    // A clone with `balls/tasks` on its own git but no `.balls/worktree`
    // checkout — the in-flight state mid-migration, or a bare-cloned
    // hub where the branch comes along without a working tree. The
    // guard returns Ok early so `ensure` proceeds to the cold-path
    // adoption.
    let d = TempDir::new().unwrap();
    let p = d.path();
    git_run(p, &["init", "-q", "-b", "main"]);
    git_run(p, &["config", "user.email", "t@e.x"]);
    git_run(p, &["config", "user.name", "t"]);
    fs::write(p.join("code.txt"), "x\n").unwrap();
    git_run(p, &["add", "code.txt"]);
    git_run(p, &["commit", "-qm", "code", "--no-verify"]);
    git_state::create_orphan_branch(p, "balls/tasks", "balls state").unwrap();
    assert!(!p.join(".balls/worktree").exists(), "no legacy worktree");

    let dir = ensure(p, &implicit()).unwrap();
    assert!(
        dir.join(".git").exists(),
        "state-repo materialized despite the missing legacy worktree"
    );
}
