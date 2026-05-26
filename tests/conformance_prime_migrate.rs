//! Phase 3 (bl-05e5) prime / migrate / doctor gates layered atop the
//! §14 conformance set:
//!
//! - `bl prime` on a legacy clone emits a specific warning naming the
//!   marker (not the generic "legacy layout in use" line Phase 1
//!   shipped).
//! - `bl prime --migrate` on a legacy clone runs `bl migrate` after
//!   the normal prime body and lands the §14.21 post-state.
//! - `bl prime --migrate` on an XDG-mode clone is a no-op.
//! - `bl doctor` against a legacy clone surfaces the migration as a
//!   finding (read-only).
//!
//! Per SPEC-tracker-state §16.13, these are new-side assertions — no
//! pinned old binaries.

mod common;

use balls::encoding::{canonicalize_origin, nested_clone_path, percent_encode_component};
use balls::xdg_paths::{own_tracker_checkout, PerClonePaths};
use common::migrate::{bl_xdg, legacy_clone};
use common::*;
use std::path::{Path, PathBuf};

#[test]
fn prime_warning_on_legacy_clone_names_the_specific_marker() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");
    let out = bl_xdg(&clone, home.path()).arg("prime").output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("legacy layout in use"));
    assert!(stderr.contains(".balls/config.json"), "warning must name marker: {stderr}");
    assert!(stderr.contains("bl prime --migrate"), "warning must offer the off-ramp: {stderr}");
}

#[test]
fn prime_migrate_on_legacy_clone_relocates_to_xdg() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");
    bl_xdg(&clone, home.path())
        .args(["prime", "--migrate"])
        .assert()
        .success();

    // SPEC §14.21 post-state: no .balls/ at clone root, migration
    // commit on main.
    assert!(!clone.join(".balls").exists(), ".balls/ must be gone");
    let head = git(&clone, &["log", "-1", "--format=%s"]);
    assert!(
        head.contains("balls: migrate to XDG layout"),
        "migration commit missing: {head}"
    );
}

#[test]
fn prime_migrate_on_already_xdg_is_a_noop() {
    let home = tmp();
    let (_remote, clone, origin_url) = legacy_clone(home.path(), "dev/proj");
    bl_xdg(&clone, home.path()).arg("migrate").assert().success();

    // Settle one-shot prime side effects (the per-clone breadcrumb
    // back-fill at sync.rs:134) so a subsequent invocation can pin
    // every surface the migrate body would touch. SPEC §14.21 makes
    // the contract on the migrate body: "Re-running `bl migrate` is a
    // no-op." Prime's own first-time writes are out of scope.
    bl_xdg(&clone, home.path())
        .args(["prime", "--migrate"])
        .assert()
        .success();

    let bases = common::migrate::bases(home.path());
    let enc_origin = percent_encode_component(&canonicalize_origin(&origin_url));
    let tracker = own_tracker_checkout(&bases, &enc_origin);
    let per = PerClonePaths::new(&bases, &nested_clone_path(&clone));

    let head_before = git(&clone, &["rev-parse", "HEAD"]).trim().to_string();
    let state_tip_before = git(&tracker, &["rev-parse", "refs/heads/balls/tasks"])
        .trim()
        .to_string();
    let per_clone_before = snapshot_per_clone(&per);

    let out = bl_xdg(&clone, home.path())
        .args(["prime", "--migrate"])
        .output()
        .unwrap();
    assert!(out.status.success(), "re-run failed: {}", String::from_utf8_lossy(&out.stderr));

    assert_eq!(
        head_before,
        git(&clone, &["rev-parse", "HEAD"]).trim(),
        "prime --migrate must add no commit on an XDG clone"
    );
    assert_eq!(
        state_tip_before,
        git(&tracker, &["rev-parse", "refs/heads/balls/tasks"]).trim(),
        "prime --migrate must not advance the state-branch tip"
    );
    assert_eq!(
        per_clone_before,
        snapshot_per_clone(&per),
        "prime --migrate must not mutate the XDG per-clone subtree"
    );
}

#[test]
fn doctor_on_legacy_clone_names_the_migration() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");
    let out = bl_xdg(&clone, home.path()).arg("doctor").output().unwrap();
    assert!(out.status.success(), "doctor must exit 0 (read-only)");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(stdout.contains("legacy layout in use"), "doctor must surface legacy layout: {stdout}");
    assert!(stdout.contains("bl prime --migrate"), "doctor must name the fix: {stdout}");
}

#[test]
fn doctor_on_freshly_migrated_clone_is_silent() {
    // bl-a4d0 conformance: once a clone is on the XDG layout, doctor
    // emits "no problems detected" — the pre-fix false positives
    // (repo.json schema vs Config, .balls/tasks symlink check) must
    // not fire on an XDG-mode store.
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");
    bl_xdg(&clone, home.path()).arg("migrate").assert().success();
    let out = bl_xdg(&clone, home.path()).arg("doctor").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("no problems detected"),
        "freshly-migrated XDG clone must read clean: {stdout}"
    );
}

#[test]
fn doctor_on_migrated_clone_with_corrupt_repo_json_flags_it() {
    // bl-a4d0 conformance: corrupting `repo.json` on the tracker
    // checkout of a migrated XDG clone surfaces as a doctor finding
    // naming the file and pointing at the tracker recovery path.
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");
    bl_xdg(&clone, home.path()).arg("migrate").assert().success();
    // Locate the tracker checkout's repo.json via XDG bases.
    let bases = common::migrate::bases(home.path());
    let trackers = bases.state_root().join("trackers");
    let repo_json = find_repo_json(&trackers).expect("repo.json on tracker checkout");
    std::fs::write(&repo_json, "{ not json").unwrap();

    let out = bl_xdg(&clone, home.path()).arg("doctor").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(stdout.contains("repo.json"), "doctor must name repo.json: {stdout}");
    assert!(stdout.contains("is unreadable"), "doctor must say unreadable: {stdout}");
    assert!(stdout.contains("tracker"), "doctor must point at the tracker branch: {stdout}");
}

fn find_repo_json(root: &Path) -> Option<PathBuf> {
    for e in std::fs::read_dir(root).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(hit) = find_repo_json(&p) {
                return Some(hit);
            }
        } else if p.file_name().is_some_and(|n| n == "repo.json") {
            return Some(p);
        }
    }
    None
}

/// Deterministic snapshot of the four per-clone roots (claims,
/// worktrees, locks, plugins-auth) by relative path + file size.
/// Catches any add/remove/grow under the per-clone subtree without
/// touching the tracker checkout's `.git/` (which `prime`'s fetch is
/// allowed to mutate).
fn snapshot_per_clone(per: &PerClonePaths) -> Vec<(&'static str, Vec<(PathBuf, u64)>)> {
    vec![
        ("claims", snapshot_dir(&per.claims)),
        ("worktrees", snapshot_dir(&per.worktrees)),
        ("locks", snapshot_dir(&per.locks)),
        ("plugins_auth", snapshot_dir(&per.plugins_auth)),
    ]
}

fn snapshot_dir(root: &Path) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    if root.exists() {
        walk(root, root, &mut out);
        out.sort();
    }
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap().to_path_buf();
        let Ok(md) = entry.metadata() else { continue };
        if md.is_dir() {
            out.push((rel, 0));
            walk(root, &path, out);
        } else {
            out.push((rel, md.len()));
        }
    }
}

#[test]
fn doctor_on_legacy_is_read_only() {
    let home = tmp();
    let (_remote, clone, _url) = legacy_clone(home.path(), "dev/proj");
    let before = std::fs::read(clone.join(".balls/config.json")).unwrap();
    bl_xdg(&clone, home.path()).arg("doctor").assert().success();
    let after = std::fs::read(clone.join(".balls/config.json")).unwrap();
    assert_eq!(before, after, "doctor must not mutate .balls/config.json");
    // The .balls/ tree itself must still be present.
    assert!(clone.join(".balls/state-repo/.git").exists());
}
