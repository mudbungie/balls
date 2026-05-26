//! SPEC-clone-layout §14 test 14 — `bl doctor` detects a moved clone
//! and `bl repair --rebind-path` relocates the orphaned per-clone
//! subtree with no data loss. New-side assertion per SPEC-tracker-state
//! §16.13: the test stands up the old + new XDG state with the
//! breadcrumb mechanism Phase 3 (bl-05e5) ships, then exercises both
//! the read-only diagnostic and the action verb.

mod common;

use balls::clone_breadcrumb;
use balls::xdg_paths::PerClonePaths;
use common::migrate::{bases, bl_xdg, legacy_clone, nested};
use common::*;
use std::fs;

/// Stand up the moved-clone scenario by hand: a migrated XDG clone
/// that recorded its path at one location, then was `mv`d. The
/// post-state mirrors what would happen in the wild — old per-clone
/// subtree retained under the previous nested path, new clone path
/// has no per-clone state yet, the breadcrumb still records the old
/// path (bl never ran at the new location to update it).
fn seed_moved_clone(
    home: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf, PerClonePaths) {
    let (_remote, clone, _url) = legacy_clone(home, "dev/proj");
    bl_xdg(&clone, home).arg("migrate").assert().success();
    let xdg_bases = bases(home);
    let old_per_clone = PerClonePaths::new(&xdg_bases, &nested(&clone));
    // Drop a sample claim file under the old subtree so the rebind
    // surfaces a non-empty `moved` list and the doctor finding names
    // a task id.
    fs::write(old_per_clone.claims.join("bl-cafe"), "x").unwrap();

    // Simulate the move: rename the clone on disk and leave the
    // breadcrumb untouched. The breadcrumb still records the OLD
    // path — that *is* the moved-from-here signal doctor surfaces.
    let new_clone = home.join("dev/proj-renamed");
    fs::rename(&clone, &new_clone).unwrap();
    (new_clone, clone, old_per_clone)
}

#[test]
fn spec_14_14_doctor_reports_moved_clone_with_task_ids_and_rebind_hint() {
    let home = tmp();
    let (new_clone, _old_clone, _old) = seed_moved_clone(home.path());

    let out = bl_xdg(&new_clone, home.path()).arg("doctor").output().unwrap();
    assert!(out.status.success(), "doctor must exit 0 (read-only)");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(stdout.contains("moved clone detected"), "missing finding: {stdout}");
    assert!(stdout.contains("bl-cafe"), "orphan task id missing: {stdout}");
    assert!(stdout.contains("bl repair --rebind-path"), "rebind hint missing: {stdout}");
}

#[test]
fn spec_14_14_doctor_is_read_only_on_moved_clone() {
    let home = tmp();
    let (new_clone, old_clone, old) = seed_moved_clone(home.path());
    let claim_before = fs::read(old.claims.join("bl-cafe")).unwrap();
    bl_xdg(&new_clone, home.path()).arg("doctor").assert().success();
    let claim_after = fs::read(old.claims.join("bl-cafe")).unwrap();
    assert_eq!(claim_before, claim_after, "doctor must not mutate orphan state");
    // The pre-doctor breadcrumb at the old location must be intact —
    // still recording the OLD clone path. Doctor never rewrites it;
    // only the rebind verb does.
    let bc = clone_breadcrumb::read_at(&old.claims).expect("breadcrumb survives doctor");
    assert_eq!(bc.path, old_clone.to_string_lossy().to_string());
}

#[test]
fn spec_14_14_rebind_moves_per_clone_subtree_with_no_data_loss() {
    let home = tmp();
    let (new_clone, _old_clone, old) = seed_moved_clone(home.path());
    let xdg_bases = bases(home.path());
    let new_per_clone = PerClonePaths::new(&xdg_bases, &nested(&new_clone));

    bl_xdg(&new_clone, home.path())
        .args(["repair", "--rebind-path"])
        .assert()
        .success();

    // (a) Data preserved at the new nested path.
    assert!(new_per_clone.claims.exists(), "new claims/ should exist after rebind");
    assert!(new_per_clone.claims.join("bl-cafe").exists(), "claim file lost");

    // (b) Old subtree gone.
    assert!(!old.claims.exists(), "old claims subtree must be removed");

    // (c) Breadcrumb now lives at the new path and records new_clone.
    let bc = clone_breadcrumb::read_at(&new_per_clone.claims).expect("breadcrumb");
    assert_eq!(bc.path, new_clone.to_string_lossy().to_string());

    // (d) Doctor is quiet on the rebind itself, and emits no false-
    //     positive findings for the pre-bl-a4d0 legacy-config /
    //     state-repo checks against an XDG clone.
    let out = bl_xdg(&new_clone, home.path()).arg("doctor").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(!stdout.contains("moved clone detected"), "rebind should silence the finding: {stdout}");
    assert!(!stdout.contains("repo.json"), "XDG clone must not surface repo.json schema errors: {stdout}");
    assert!(!stdout.contains("state checkout"), "XDG clone has no state-repo to validate: {stdout}");
    assert!(!stdout.contains(".balls/tasks"), "XDG clone has no in-repo tasks symlink to chase: {stdout}");
}

