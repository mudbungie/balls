//! SPEC §14 conformance tests for the orphan-branch topology.
//!
//! Phase 1B (bl-213e) flipped `bl init` to the XDG layout: tasks now
//! live in the tracker checkout under `~/.local/state/balls/trackers/…`,
//! not at `<clone>/.balls/tasks`. The legacy "symlink + state-repo at
//! the clone root" topology is gone; the §14.x conformance gates that
//! cover XDG path layout, idempotency, and lifecycle live in
//! `tests/conformance_xdg_init.rs` and `tests/conformance_xdg_layout.rs`.
//! Only the legacy-independent stories (`main` log cleanliness through
//! a task lifecycle) survive here.

mod common;

use common::*;

/// §14.10 — Naïve visibility: a task's `.json` is readable with stock
/// tools immediately after `bl init` runs on a fresh checkout. Under
/// XDG the file lives under the tracker checkout (no symlink at the
/// clone root); the resolved path is what stock tools target.
#[test]
fn task_files_are_readable_with_stock_tools() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "visible");
    let path = discover_tasks_dir(repo.path()).join(format!("{id}.json"));
    assert!(path.exists(), "task file must be on-disk and readable");
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains(&id));
}

/// §14.11 — Main log contains ONLY feature commits during a task
/// lifecycle. No `balls: create/claim/close` noise on main.
#[test]
fn main_log_stays_clean_through_task_lifecycle() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "cleanlog");
    bl(repo.path())
        .args(["update", &id, "priority=1", "--note", "done"])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &id, "status=closed"])
        .assert()
        .success();

    let main_log = git(repo.path(), &["log", "--oneline", "main"]);
    assert!(
        !main_log.contains("create bl-"),
        "main must not carry balls: create commits: {main_log}"
    );
    assert!(
        !main_log.contains("update bl-"),
        "main must not carry balls: update commits: {main_log}"
    );
    assert!(
        !main_log.contains("close bl-"),
        "main must not carry balls: close commits: {main_log}"
    );

    // The state branch has all the bookkeeping.
    let state_log = git_state(repo.path(), &["log", "--oneline", "balls/tasks"]);
    assert!(state_log.contains("create"));
    assert!(state_log.contains("close"));
}

/// §14.14 — `bl init` is idempotent; running it twice leaves the
/// state branch tip unchanged and the tracker checkout healthy.
#[test]
fn bl_init_is_idempotent() {
    let repo = new_repo();
    init_in(repo.path());
    let state_sha_1 = git_state(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();
    init_in(repo.path());
    let state_sha_2 = git_state(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(
        state_sha_1, state_sha_2,
        "second bl init must not advance the state branch"
    );
    assert!(
        discover_state_repo(repo.path()).unwrap().join(".git").exists(),
        "tracker checkout still present after second init",
    );
}

// Retired (Phase 1B XDG flip):
//   - `bl_init_self_heals_missing_symlink`: XDG init plants no
//     `.balls/tasks` symlink at the clone root, so there is nothing
//     to heal.
//   - `bl_init_refuses_when_tasks_path_is_not_a_symlink`: same — XDG
//     init never reads or writes `<clone>/.balls/`.
//   - `bl_fails_when_state_checkout_lacks_its_tasks_dir` and
//     `missing_state_checkout_is_re_materialized`: re-materialization
//     of the tracker checkout is covered by
//     `conformance_xdg_layout::spec_14_12_xdg_state_dir_regenerable`.
//   - `claimed_worktree_shares_state_with_main`: under XDG the worktree
//     has no `.balls/` of its own (worktree.rs Layout::Xdg guard);
//     `bl <cmd>` from inside a worktree resolves the same tracker via
//     `Store::discover` walking back to the main clone root.
