//! Multi-developer scenario fixtures: the standard three-way topology
//! (a bare remote plus two cloned dev workspaces) and the small
//! remote-failure / worktree-edit helpers its sync tests share.

#![allow(dead_code)]

use super::{bl, clone_from_remote, git, new_bare_remote, push, Repo};
use std::fs;
use std::path::Path;

/// A bare remote with two initialized, cloned dev workspaces. Alice has
/// pushed `bl init`; Bob is initialized but unpushed — the standard
/// starting point for the multi-dev sync and lifecycle-sync stories.
pub fn three_way() -> (Repo, Repo, Repo) {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    bl(alice.path()).arg("init").assert().success();
    push(alice.path());

    let bob = clone_from_remote(remote.path(), "bob");
    bl(bob.path()).arg("init").assert().success();
    (remote, alice, bob)
}

/// Point a repo's `origin` — and its state checkout's `origin` — at a
/// path that does not exist, so the next remote round-trip fails.
/// Under the unified model the state branch lives in
/// `.balls/state-repo`, an independent clone with its own `origin`,
/// so a sync-failure test must break both.
pub fn break_remote(repo: &Path) {
    let bad = "/tmp/balls-no-such-remote.git";
    git(repo, &["remote", "set-url", "origin", bad]);
    let state_repo = repo.join(".balls/state-repo");
    if state_repo.join(".git").exists() {
        git(&state_repo, &["remote", "set-url", "origin", bad]);
    }
}

/// Write a placeholder source file into a claimed task's worktree so
/// the next `bl review` has a real diff to squash.
pub fn write_some_code(wt: &Path, name: &str) {
    fs::write(wt.join(name), "code\n").unwrap();
}
