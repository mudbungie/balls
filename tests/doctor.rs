//! `bl doctor` — read-only drift diagnostics. Every check, every
//! branch: a healthy repo is silent; each kind of drift names itself
//! and points at the fixing command without mutating anything.

mod common;

use common::*;
use std::fs;

#[test]
fn clean_repo_reports_only_the_pending_xdg_migration() {
    // Pre-Phase-1B, `bl init` writes the legacy layout, so the
    // SPEC-clone-layout §12 row 2 nudge from bl-05e5 always fires
    // on a fresh-init clone. The only finding is the migration hint;
    // no drift, no orphans.
    let repo = new_repo();
    init_in(repo.path());
    let out = doctor(repo.path());
    assert!(out.contains("legacy layout in use"));
    assert!(out.contains("bl prime --migrate"));
    assert!(out.contains("1 problem"));
}

#[test]
fn uninitialized_with_bl_docs_connects_them() {
    let dir = tmp();
    fs::write(dir.path().join("AGENTS.md"), "Task tracking uses bl prime.\n").unwrap();
    let out = doctor(dir.path());
    assert!(out.contains("bl is not usable here"));
    assert!(out.contains("docs reference bl"));
    assert!(out.contains("remove the bl"));
}

#[test]
fn uninitialized_without_docs_just_reports_discovery() {
    let dir = tmp();
    let out = doctor(dir.path());
    assert!(out.contains("bl is not usable here"));
    assert!(!out.contains("docs reference bl"));
    assert!(out.contains("1 problem(s)"));
}

#[test]
fn state_checkout_with_a_file_gitdir_is_flagged() {
    // The unified state checkout is a full clone — `.git` must be a
    // directory. A `.git` *file* (a stray worktree pointer) is drift.
    let repo = new_repo();
    init_in(repo.path());
    let gitdir = repo.path().join(".balls/state-repo/.git");
    fs::remove_dir_all(&gitdir).unwrap();
    fs::write(&gitdir, "garbage\n").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("not a valid git clone"), "{out}");
    assert!(out.contains("bl prime"), "{out}");
    assert!(!out.contains("bl repair"));
}

#[test]
fn state_checkout_with_no_head_is_flagged() {
    let repo = new_repo();
    init_in(repo.path());
    fs::remove_file(repo.path().join(".balls/state-repo/.git/HEAD")).unwrap();
    assert!(doctor(repo.path()).contains("not a valid git clone"));
}

#[test]
fn missing_tasks_symlink_is_flagged() {
    let repo = new_repo();
    init_in(repo.path());
    std::fs::remove_file(repo.path().join(".balls/tasks")).unwrap();
    assert!(doctor(repo.path()).contains("convenience symlink is missing"));
}

#[test]
fn stray_non_symlink_at_tasks_path_is_flagged() {
    let repo = new_repo();
    init_in(repo.path());
    std::fs::remove_file(repo.path().join(".balls/tasks")).unwrap();
    std::fs::create_dir(repo.path().join(".balls/tasks")).unwrap();
    assert!(doctor(repo.path()).contains("not a symlink"));
}

#[test]
fn stale_tasks_symlink_target_is_flagged() {
    let repo = new_repo();
    init_in(repo.path());
    let link = repo.path().join(".balls/tasks");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("worktree/.balls/tasks", &link).unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("points to") && out.contains("expected"), "{out}");
}

#[test]
fn deleted_state_checkout_rebuilds_via_doctors_hint() {
    // `.balls/state-repo` is re-materializable runtime state — the
    // doctor hint says re-run `bl prime`, and doing so self-heals.
    // The legacy-layout finding still fires (pre-Phase-1B init writes
    // legacy); the state-checkout finding does not.
    let repo = new_repo();
    init_in(repo.path());
    fs::remove_dir_all(repo.path().join(".balls/state-repo")).unwrap();
    bl(repo.path()).arg("prime").assert().success();
    let out = doctor(repo.path());
    assert!(!out.contains("not a valid git clone"));
    assert!(out.contains("legacy layout in use"));
}

#[test]
fn claim_file_with_no_task() {
    let repo = new_repo();
    init_in(repo.path());
    fs::write(repo.path().join(".balls/local/claims/bl-zzzz"), "x").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("claim file for bl-zzzz but no such task"));
    assert!(out.contains("bl repair --fix"));
}

#[test]
fn claim_file_for_task_not_in_progress() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "open task");
    fs::write(repo.path().join(".balls/local/claims").join(&id), "x").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains(&format!("claim file for {id} but its status is open")));
    assert!(out.contains("bl drop"));
}

#[test]
fn properly_claimed_task_is_silent_beyond_migration_hint() {
    // Exercises the in-progress claim arm and a worktree that *does*
    // have a matching claim — neither is drift. The legacy-layout
    // finding still fires until Phase 1B flips `bl init` to XDG.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "real work");
    bl(repo.path()).args(["claim", &id]).assert().success();
    let out = doctor(repo.path());
    assert!(!out.contains("orphan"));
    assert!(!out.contains("but no such task"));
    assert!(out.contains("legacy layout in use"));
}

#[test]
fn orphan_worktree_dir_is_flagged() {
    let repo = new_repo();
    init_in(repo.path());
    // A worktree named after a real (but unclaimed) task is NOT an
    // orphan; one with no task or claim behind it is.
    let id = create_task(repo.path(), "has a task");
    let wt = repo.path().join(".balls-worktrees");
    fs::create_dir_all(wt.join(&id)).unwrap();
    fs::create_dir_all(wt.join("bl-dead")).unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("bl-dead"));
    assert!(out.contains("has no matching claim or task"));
    assert!(!out.contains(&format!("worktree dir {}", wt.join(&id).display())));
}

#[test]
fn corrupt_config_is_flagged() {
    let repo = new_repo();
    init_in(repo.path());
    fs::write(repo.path().join(".balls/config.json"), "{ not json").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("config") && out.contains("is unreadable"));
    assert!(out.contains("git checkout main"));
}

#[test]
fn tasks_dir_override_points_nowhere() {
    let repo = new_repo();
    init_in(repo.path());
    fs::write(
        repo.path().join(".balls/local/tasks_dir"),
        "/no/such/balls/path",
    )
    .unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("tasks_dir override"));
    assert!(out.contains("/no/such/balls/path"));
}

#[test]
fn healthy_stealth_store_only_reports_migration_hint() {
    // --tasks-dir points the override at a real directory: not drift,
    // and the state-worktree check is correctly skipped for stealth.
    // Stealth `bl init` still writes `.balls/config.json`, so the
    // legacy-layout finding fires for the same reason as the
    // non-stealth case.
    let repo = new_repo();
    let ext = tmp();
    bl(repo.path())
        .args(["init", "--tasks-dir"])
        .arg(ext.path())
        .assert()
        .success();
    let out = doctor(repo.path());
    assert!(out.contains("legacy layout in use"));
    assert!(!out.contains("not a valid git clone"));
}

#[test]
fn missing_claims_dir_is_not_a_stale_claim_finding() {
    let repo = new_repo();
    init_in(repo.path());
    fs::remove_dir_all(repo.path().join(".balls/local/claims")).unwrap();
    let out = doctor(repo.path());
    assert!(!out.contains("but no such task"));
    assert!(!out.contains("but its status"));
    // The legacy-layout migration hint still fires; that's expected.
}

// master_url-mode probes live in tests/doctor_master_url.rs — split
// out for the file-size budget. Legacy and structural probes stay here.
