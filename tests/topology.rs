//! SPEC §14 conformance tests for the orphan-branch topology.

mod common;

use common::*;

/// §14.10 — Naïve visibility: `.balls/tasks/<id>.json` is readable with
/// stock tools immediately after `bl init` runs on a fresh checkout.
#[test]
fn symlink_exposes_tasks_to_stock_tools() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "visible");
    let path = repo.path().join(".balls/tasks").join(format!("{}.json", id));
    assert!(path.exists(), "task file must be reachable via symlink");
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains(&id));
    // The `.balls/tasks` path in main's tree is a symlink into the
    // state worktree, not a real directory.
    assert!(repo.path().join(".balls/tasks").is_symlink());
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
        "main must not carry balls: create commits: {}",
        main_log
    );
    assert!(
        !main_log.contains("update bl-"),
        "main must not carry balls: update commits: {}",
        main_log
    );
    assert!(
        !main_log.contains("close bl-"),
        "main must not carry balls: close commits: {}",
        main_log
    );

    // The state branch has all the bookkeeping.
    let state_log = git(repo.path(), &["log", "--oneline", "balls/tasks"]);
    assert!(state_log.contains("create"));
    assert!(state_log.contains("close"));
}

/// §14.14 — `bl init` is idempotent; running it twice leaves a healthy
/// state and the second run is a no-op on the state branch.
#[test]
fn bl_init_is_idempotent() {
    let repo = new_repo();
    init_in(repo.path());
    let state_sha_1 = git(
        repo.path(),
        &["rev-parse", "refs/heads/balls/tasks"],
    )
    .trim()
    .to_string();

    // Running init again must not advance the state branch or break
    // the symlink.
    init_in(repo.path());
    let state_sha_2 = git(
        repo.path(),
        &["rev-parse", "refs/heads/balls/tasks"],
    )
    .trim()
    .to_string();
    assert_eq!(
        state_sha_1, state_sha_2,
        "second bl init must not advance the state branch"
    );
    assert!(repo.path().join(".balls/tasks").is_symlink());
    assert!(repo.path().join(".balls/worktree").exists());
}

/// §14.15 — `bl init` self-heals a missing symlink or worktree. If a
/// user deletes the symlink by accident, re-running init restores it.
#[test]
fn bl_init_self_heals_missing_symlink() {
    let repo = new_repo();
    init_in(repo.path());
    // Remove the symlink to simulate damage.
    std::fs::remove_file(repo.path().join(".balls/tasks")).unwrap();
    assert!(!repo.path().join(".balls/tasks").exists());
    init_in(repo.path());
    assert!(repo.path().join(".balls/tasks").is_symlink());
}

/// Bl worktrees created by `bl claim` inherit the state symlink so a
/// worker sees the same task state as main without any additional
/// setup.
#[test]
fn claimed_worktree_shares_state_with_main() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "shared view");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
    // The bl worktree's .balls/tasks symlink must resolve to a file
    // containing this task — proving it targets the same state.
    let task_path_via_wt = wt.join(".balls/tasks").join(format!("{}.json", id));
    assert!(
        task_path_via_wt.exists(),
        "bl worktree's .balls/tasks symlink must expose the task"
    );
    let contents = std::fs::read_to_string(&task_path_via_wt).unwrap();
    assert!(contents.contains(&id));
}
