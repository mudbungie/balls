//! `bl doctor` — read-only drift diagnostics. Every check, every
//! branch: a healthy repo is silent; each kind of drift names itself
//! and points at the fixing command without mutating anything.
//!
//! Tests come in two flavors. **XDG-mode** tests use [`new_xdg_repo`]
//! and exercise the layout the binary writes after Phase 1B-5 (the
//! `cmd_init` flip). **Legacy-mode** tests use [`legacy_clone`] to
//! hand-scaffold a pre-XDG fixture; these preserve coverage for the
//! state-checkout, tasks-symlink, and stealth-marker drifts that only
//! fire on the legacy layout (doctor early-returns from
//! `check_state_repo` and `check_tasks_symlink` for XDG, per
//! SPEC-clone-layout §7).

mod common;

use common::*;
use std::fs;

// ---- XDG-layout fixtures (post Phase 1B-5 shape) ----

#[test]
fn clean_xdg_repo_is_silent() {
    // XDG `bl init` writes no legacy markers, so doctor finds nothing.
    // This is the post-Phase-1B-5 successor to the pre-flip
    // "legacy layout in use" hint test.
    let repo = new_xdg_repo();
    let out = doctor(repo.path());
    assert!(out.contains("no problems detected"), "{out}");
}

#[test]
fn claim_file_with_no_task() {
    let repo = new_xdg_repo();
    fs::write(claims_dir(repo.path()).join("bl-zzzz"), "x").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("claim file for bl-zzzz but no such task"));
    assert!(out.contains("bl repair --fix"));
}

#[test]
fn claim_file_for_task_not_in_progress() {
    let repo = new_xdg_repo();
    let id = create_task(repo.path(), "open task");
    fs::write(claims_dir(repo.path()).join(&id), "x").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains(&format!("claim file for {id} but its status is open")));
    assert!(out.contains("bl drop"));
}

#[test]
fn properly_claimed_task_is_silent() {
    // An in-progress claim with a matching worktree is healthy under
    // XDG — no orphan, no stale-claim finding, no legacy markers.
    let repo = new_xdg_repo();
    let id = create_task(repo.path(), "real work");
    bl(repo.path()).args(["claim", &id]).assert().success();
    let out = doctor(repo.path());
    assert!(!out.contains("orphan"), "{out}");
    assert!(!out.contains("but no such task"), "{out}");
    assert!(!out.contains("legacy layout in use"), "{out}");
}

#[test]
fn orphan_worktree_dir_is_flagged() {
    let repo = new_xdg_repo();
    // A worktree named after a real (but unclaimed) task is NOT an
    // orphan; one with no task or claim behind it is.
    let id = create_task(repo.path(), "has a task");
    let real_wt = worktree_path(repo.path(), &id);
    fs::create_dir_all(&real_wt).unwrap();
    fs::create_dir_all(worktree_path(repo.path(), "bl-dead")).unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("bl-dead"));
    assert!(out.contains("has no matching claim or task"));
    assert!(!out.contains(&format!("worktree dir {}", real_wt.display())));
}

#[test]
fn corrupt_repo_json_is_flagged() {
    // Under XDG the per-repo config lives on the tracker branch at
    // `<tracker>/.balls/repo.json`; doctor names the file and points
    // at the tracker-checkout git history for restore.
    let repo = new_xdg_repo();
    fs::write(config_path(repo.path()), "{ not json").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("repo.json") && out.contains("is unreadable"));
    assert!(out.contains("tracker") && out.contains("git history"));
}

#[test]
fn missing_claims_dir_is_not_a_stale_claim_finding() {
    let repo = new_xdg_repo();
    fs::remove_dir_all(claims_dir(repo.path())).unwrap();
    let out = doctor(repo.path());
    assert!(!out.contains("but no such task"));
    assert!(!out.contains("but its status"));
}

#[test]
fn legacy_local_tasks_dir_marker_is_flagged_even_on_xdg() {
    // bl-5a03 retired the `.balls/local/tasks_dir` reader. Doctor
    // surfaces the file as a pre-XDG marker so the user knows to move
    // the value into `clone.json.tasks_dir`. Layout-agnostic — the
    // probe walks `<clone>/.balls/local/tasks_dir` regardless of the
    // resolved store layout. Asserting on an XDG fixture proves the
    // detection survives the 1B-5 flip.
    let repo = new_xdg_repo();
    fs::create_dir_all(repo.path().join(".balls/local")).unwrap();
    fs::write(
        repo.path().join(".balls/local/tasks_dir"),
        "/no/such/balls/path",
    )
    .unwrap();
    let out = doctor(repo.path());
    assert!(
        out.contains(".balls/local/tasks_dir") && out.contains("pre-XDG"),
        "expected legacy-marker finding for local/tasks_dir; got:\n{out}"
    );
}

#[test]
fn legacy_local_config_marker_is_flagged_even_on_xdg() {
    let repo = new_xdg_repo();
    fs::create_dir_all(repo.path().join(".balls/local")).unwrap();
    fs::write(
        repo.path().join(".balls/local/config.json"),
        r#"{"require_remote_on_claim": false}"#,
    )
    .unwrap();
    let out = doctor(repo.path());
    assert!(
        out.contains(".balls/local/config.json") && out.contains("pre-XDG"),
        "expected legacy-marker finding for local/config.json; got:\n{out}"
    );
}

#[test]
fn legacy_pending_sync_dir_with_files_is_flagged_even_on_xdg() {
    // bl-341b: the standalone `pending_sync_legacy::warn_if_present`
    // hook on every `bl` invocation was retired. `bl doctor` surfaces
    // a populated `<repo>/.balls/local/pending-sync/` tree on demand.
    // Layout-agnostic probe — verified on an XDG fixture.
    let repo = new_xdg_repo();
    let staged = repo.path().join(".balls/local/pending-sync/sync");
    fs::create_dir_all(&staged).unwrap();
    fs::write(staged.join("abcd.json"), b"{}").unwrap();
    let out = doctor(repo.path());
    assert!(out.contains("1 staged sync reports"), "{out}");
    assert!(out.contains(".balls/local/pending-sync"), "{out}");
    assert!(out.contains("bl-6969"), "{out}");
    assert!(
        staged.join("abcd.json").exists(),
        "doctor is read-only — the staged report must survive"
    );
}

#[test]
fn empty_legacy_pending_sync_dir_is_silent() {
    let repo = new_xdg_repo();
    fs::create_dir_all(repo.path().join(".balls/local/pending-sync/sync")).unwrap();
    let out = doctor(repo.path());
    assert!(!out.contains("staged sync reports"), "{out}");
}

// ---- Layout-independent fixtures ----

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

// ---- Legacy-layout fixtures (drift that fires only on the
// pre-XDG state checkout + tasks symlink) ----

#[test]
fn legacy_state_checkout_with_a_file_gitdir_is_flagged() {
    // The unified state checkout is a full clone — `.git` must be a
    // directory. A `.git` *file* (a stray worktree pointer) is drift.
    // XDG mode has no state checkout at the clone root, so this drift
    // is legacy-only.
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "proj");
    let gitdir = discover_state_repo(&clone).unwrap().join(".git");
    fs::remove_dir_all(&gitdir).unwrap();
    fs::write(&gitdir, "garbage\n").unwrap();
    let out = doctor(&clone);
    assert!(out.contains("not a valid git clone"), "{out}");
    assert!(out.contains("bl prime"), "{out}");
    assert!(!out.contains("bl repair"));
}

#[test]
fn legacy_state_checkout_with_no_head_is_flagged() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "proj");
    fs::remove_file(discover_state_repo(&clone).unwrap().join(".git/HEAD")).unwrap();
    assert!(doctor(&clone).contains("not a valid git clone"));
}

#[test]
fn legacy_missing_tasks_symlink_is_flagged() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "proj");
    fs::remove_file(clone.join(".balls/tasks")).unwrap();
    assert!(doctor(&clone).contains("convenience symlink is missing"));
}

#[test]
fn legacy_stray_non_symlink_at_tasks_path_is_flagged() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "proj");
    fs::remove_file(clone.join(".balls/tasks")).unwrap();
    fs::create_dir(clone.join(".balls/tasks")).unwrap();
    assert!(doctor(&clone).contains("not a symlink"));
}

#[test]
fn legacy_stale_tasks_symlink_target_is_flagged() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "proj");
    let link = clone.join(".balls/tasks");
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("worktree/.balls/tasks", &link).unwrap();
    let out = doctor(&clone);
    assert!(out.contains("points to") && out.contains("expected"), "{out}");
}

#[test]
fn legacy_corrupt_config_is_flagged() {
    // Pre-XDG corrupt `.balls/config.json` — the `check_config_legacy`
    // branch of `doctor_config`. Under XDG the per-repo config lives at
    // `<tracker>/.balls/repo.json` and the diagnostic differs (see
    // `corrupt_repo_json_is_flagged`); the legacy branch's "restore
    // with `git checkout main`" hint stays covered here. Also walks
    // the `worktrees_root() -> Err` early-return in
    // `check_orphan_worktrees` (legacy `worktrees_root` loads
    // `Config`, which fails when the file is garbage).
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "proj");
    fs::write(clone.join(".balls/config.json"), "{ not json").unwrap();
    let out = doctor(&clone);
    assert!(out.contains("config") && out.contains("is unreadable"));
    assert!(out.contains("git checkout main"));
}

#[test]
fn legacy_deleted_state_checkout_rebuilds_via_doctors_hint() {
    // `.balls/state-repo` is re-materializable runtime state — the
    // doctor hint says re-run `bl prime`, and doing so self-heals.
    // The legacy-layout finding still fires (the clone is on the
    // legacy layout); the state-checkout finding does not.
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "proj");
    fs::remove_dir_all(discover_state_repo(&clone).unwrap()).unwrap();
    bl(&clone).arg("prime").assert().success();
    let out = doctor(&clone);
    assert!(!out.contains("not a valid git clone"));
    assert!(out.contains("legacy layout in use"));
}

// ---- XDG stealth ----

#[test]
fn healthy_xdg_stealth_store_is_silent() {
    // `bl init --tasks-dir` in XDG mode writes `clone.json` and no
    // tracker checkout. No legacy markers, no state-repo to validate.
    // Pre-Phase-1B-5 successor to the legacy-stealth migration-hint
    // assertion: XDG stealth is clean from inception.
    let repo = new_repo();
    let ext = tmp();
    bl(repo.path())
        .args(["init", "--tasks-dir"])
        .arg(ext.path())
        .assert()
        .success();
    let out = doctor(repo.path());
    assert!(!out.contains("not a valid git clone"), "{out}");
}

// master_url-mode probes live in tests/doctor_master_url.rs — split
// out for the file-size budget. Legacy and structural probes stay here.
