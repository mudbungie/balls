//! bl-8cf7: read-only/root commands must work from a bare repo root.
//!
//! The recommended balls layout is a bare central hub (`core.bare =
//! true`) with work in linked `.balls-worktrees/<id>/` checkouts.
//! `Store::discover` used to bail when `rev-parse --show-toplevel`
//! failed (it always fails with no work tree) and fall through to
//! no-git discovery, surfacing a misleading "not initialized. Run
//! `bl init`" on a fully-initialized repo. A bare root must instead
//! resolve to the gitdir's parent and operate normally.

mod common;

use common::*;
use predicates::prelude::*;

/// Flip the main repo's `core.bare` flag directly, mimicking a
/// bare-converted hub without going through `bl`.
fn set_core_bare(repo_root: &std::path::Path, bare: bool) {
    git(
        repo_root,
        &["config", "core.bare", if bare { "true" } else { "false" }],
    );
}

#[test]
fn list_succeeds_from_bare_repo_root() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "visible from bare root");

    set_core_bare(repo.path(), true);

    // Sanity: the bare flag really took — git itself now refuses a
    // work-tree command from the root, which is exactly the condition
    // that used to mislead discovery.
    assert!(
        !git_ok(repo.path(), &["status"]),
        "core.bare should make `git status` fail at the root"
    );

    bl(repo.path())
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&id));
}
