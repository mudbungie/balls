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
/// has no per-clone state yet, the recorded breadcrumb names the new
/// path because that is what the user moved *to*.
fn seed_moved_clone(
    home: &std::path::Path,
) -> (std::path::PathBuf, PerClonePaths) {
    let (_remote, clone, _url) = legacy_clone(home, "dev/proj");
    bl_xdg(&clone, home).arg("migrate").assert().success();
    let xdg_bases = bases(home);
    let old_per_clone = PerClonePaths::new(&xdg_bases, &nested(&clone));
    // Drop a sample claim file under the old subtree so the rebind
    // surfaces a non-empty `moved` list and the doctor finding names
    // a task id.
    fs::write(old_per_clone.claims.join("bl-cafe"), "x").unwrap();

    // Simulate the move: rename the clone on disk, then rewrite the
    // breadcrumb to record the new path (matching what bl would do
    // on the *next* materialization at the new path).
    let new_clone = home.join("dev/proj-renamed");
    fs::rename(&clone, &new_clone).unwrap();
    clone_breadcrumb::write_at(&old_per_clone.claims, &new_clone).unwrap();
    (new_clone, old_per_clone)
}

#[test]
fn spec_14_14_doctor_reports_moved_clone_with_task_ids_and_rebind_hint() {
    let home = tmp();
    let (new_clone, _old) = seed_moved_clone(home.path());

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
    let (new_clone, old) = seed_moved_clone(home.path());
    let claim_before = fs::read(old.claims.join("bl-cafe")).unwrap();
    bl_xdg(&new_clone, home.path()).arg("doctor").assert().success();
    let claim_after = fs::read(old.claims.join("bl-cafe")).unwrap();
    assert_eq!(claim_before, claim_after, "doctor must not mutate orphan state");
    // The pre-doctor breadcrumb at the old location must be intact
    // (post-doctor read of the same location returns the same path).
    let bc = clone_breadcrumb::read_at(&old.claims).expect("breadcrumb survives doctor");
    assert_eq!(bc.path, new_clone.to_string_lossy().to_string());
}

#[test]
fn spec_14_14_rebind_moves_per_clone_subtree_with_no_data_loss() {
    let home = tmp();
    let (new_clone, old) = seed_moved_clone(home.path());
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

    // (d) Doctor is quiet on the migration after rebind — no more
    //     moved-clone finding (the legacy-layout finding is *not*
    //     fired against an XDG-mode clone, so post-migrate-post-rebind
    //     reads as silent).
    let out = bl_xdg(&new_clone, home.path()).arg("doctor").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(!stdout.contains("moved clone detected"), "rebind should silence the finding: {stdout}");
}

