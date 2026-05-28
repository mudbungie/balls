//! Multi-developer scenario fixtures: the standard three-way topology
//! (a bare remote plus two cloned dev clones) and the small
//! remote-failure / worktree-edit helpers its sync tests share.

#![allow(dead_code)]

use super::{bl, clone_from_remote, discover_state_repo, git, new_bare_remote, push, Repo};
use std::fs;
use std::path::Path;

/// A bare remote with two initialized, cloned dev clones. Alice has
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

/// Break the remote round-trip without changing the clone's `origin`
/// URL. The clone's `origin` keys XDG discovery (`<enc-origin>` →
/// tracker checkout); changing it would point discovery at a tracker
/// that was never materialized and break `bl <cmd>` before the sync
/// path even runs. Only the state checkout's `origin` is repointed at
/// a bogus path so push/fetch on `balls/tasks` fails — the lifecycle
/// commands keep resolving via the warm tracker checkout.
pub fn break_remote(repo: &Path) {
    let bad = "/tmp/balls-no-such-remote.git";
    if let Some(state_repo) = discover_state_repo(repo) {
        if state_repo.join(".git").exists() {
            git(&state_repo, &["remote", "set-url", "origin", bad]);
        }
    }
}

/// Write a placeholder source file into a claimed task's worktree so
/// the next `bl review` has a real diff to squash.
pub fn write_some_code(wt: &Path, name: &str) {
    fs::write(wt.join(name), "code\n").unwrap();
}
