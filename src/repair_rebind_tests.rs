use super::*;
use crate::clone_breadcrumb;
use crate::xdg_paths::{PerClonePaths, XdgBases};
use std::fs;
use tempfile::TempDir;

fn tmp() -> TempDir {
    tempfile::Builder::new().prefix("balls-rebind-").tempdir().unwrap()
}

fn bases_at(home: &Path) -> XdgBases {
    XdgBases::with(home, None, None, None)
}

/// Set up a moved-clone scenario: state lives under `old_nested/`,
/// the clone is now at `clone_path`, and the breadcrumb at the old
/// nested path records a never-existed "phantom" old path under
/// `home` so the disambiguation gate in [`find_orphans`] classifies
/// it as moved-away-from-here. Returns `(clone_path, old per-clone
/// paths, new per-clone paths)`.
fn seed_moved(
    home: &Path,
    old_nested: &str,
    clone_rel: &str,
) -> (PathBuf, PerClonePaths, PerClonePaths) {
    let bases = bases_at(home);
    let clone = home.join(clone_rel);
    fs::create_dir_all(&clone).unwrap();
    let old = PerClonePaths::new(&bases, std::path::Path::new(old_nested));
    fs::create_dir_all(&old.claims).unwrap();
    fs::create_dir_all(&old.locks).unwrap();
    fs::create_dir_all(&old.worktrees).unwrap();
    fs::create_dir_all(&old.plugins_auth).unwrap();
    let phantom_old = home.join("phantom-old").join(old_nested);
    let bc = clone_breadcrumb::CloneBreadcrumb {
        path: phantom_old.to_string_lossy().into_owned(),
        hostname: clone_breadcrumb::hostname(),
    };
    fs::write(
        clone_breadcrumb::breadcrumb_path(&old.claims),
        serde_json::to_string(&bc).unwrap(),
    )
    .unwrap();
    let new_nested = nested_clone_path(&clone);
    let new = PerClonePaths::new(&bases, &new_nested);
    (clone, old, new)
}

#[test]
fn no_orphans_returns_empty() {
    let home = tmp();
    let bases = bases_at(home.path());
    let clone = home.path().join("dev/here");
    fs::create_dir_all(&clone).unwrap();
    let reports = run_with(&bases, &clone).unwrap();
    assert!(reports.is_empty());
}

#[test]
fn rebind_moves_all_four_siblings() {
    let home = tmp();
    let bases = bases_at(home.path());
    let (clone, old, new) = seed_moved(home.path(), "home/u/old", "dev/here");
    fs::write(old.claims.join("bl-aaaa"), "x").unwrap();
    fs::write(old.locks.join("bl-aaaa"), "x").unwrap();
    fs::create_dir_all(old.worktrees.join("bl-aaaa")).unwrap();
    fs::write(old.plugins_auth.join("token"), "secret").unwrap();

    let reports = run_with(&bases, &clone).unwrap();
    assert_eq!(reports.len(), 1);
    let r = &reports[0];
    assert_eq!(r.nested_from, std::path::PathBuf::from("home/u/old"));
    assert_eq!(r.moved.len(), 4);

    assert!(new.claims.join("bl-aaaa").exists());
    assert!(new.locks.join("bl-aaaa").exists());
    assert!(new.worktrees.join("bl-aaaa").exists());
    assert!(new.plugins_auth.join("token").exists());

    assert!(!old.claims.exists());
    assert!(!old.locks.exists());
    assert!(!old.worktrees.exists());
    assert!(!old.plugins_auth.exists());

    // Breadcrumb at the new location records the current clone.
    let bc = clone_breadcrumb::read_at(&new.claims).unwrap();
    assert_eq!(bc.path, clone.to_string_lossy().to_string());
}

#[test]
fn rebind_skips_siblings_that_dont_exist() {
    let home = tmp();
    let bases = bases_at(home.path());
    let (clone, old, new) = seed_moved(home.path(), "home/u/old", "dev/here");
    fs::write(old.claims.join("bl-aaaa"), "x").unwrap();
    fs::remove_dir(&old.locks).unwrap();
    fs::remove_dir(&old.worktrees).unwrap();
    fs::remove_dir(&old.plugins_auth).unwrap();

    let reports = run_with(&bases, &clone).unwrap();
    assert_eq!(reports[0].moved.len(), 1);
    assert!(new.claims.exists());
    assert!(!new.locks.exists());
}

#[test]
fn rebind_refuses_when_destination_has_content() {
    let home = tmp();
    let bases = bases_at(home.path());
    let (clone, old, new) = seed_moved(home.path(), "home/u/old", "dev/here");
    fs::write(old.claims.join("bl-aaaa"), "x").unwrap();
    fs::create_dir_all(&new.claims).unwrap();
    fs::write(new.claims.join("bl-conflict"), "x").unwrap();

    let err = run_with(&bases, &clone).unwrap_err();
    assert!(err.to_string().contains("refusing rebind"));
    // Source untouched.
    assert!(old.claims.exists());
}

#[test]
fn rebind_succeeds_over_empty_destination_placeholders() {
    // Routine `bl` invocations mkdir the per-clone tree at the new
    // nested path (SPEC §7 step 7) before doctor is run. The rebind
    // must tolerate empty placeholders.
    let home = tmp();
    let bases = bases_at(home.path());
    let (clone, old, new) = seed_moved(home.path(), "home/u/old", "dev/here");
    fs::write(old.claims.join("bl-aaaa"), "x").unwrap();
    fs::create_dir_all(&new.claims).unwrap();
    fs::create_dir_all(&new.locks).unwrap();

    let reports = run_with(&bases, &clone).unwrap();
    assert!(!reports.is_empty());
    assert!(new.claims.join("bl-aaaa").exists());
}

#[test]
fn rebind_surfaces_rename_failure_with_detail() {
    // A regular file at the destination evades has_content (read_dir
    // errors → false) and survives the silent remove_dir (also
    // errors), then trips fs::rename. The error message names both
    // ends so the user can recover by hand.
    let home = tmp();
    let bases = bases_at(home.path());
    let (clone, old, new) = seed_moved(home.path(), "home/u/old", "dev/here");
    fs::write(old.claims.join("bl-aaaa"), "x").unwrap();
    fs::create_dir_all(new.claims.parent().unwrap()).unwrap();
    fs::write(&new.claims, "i am a file, not a dir").unwrap();

    let err = run_with(&bases, &clone).unwrap_err();
    let s = err.to_string();
    assert!(s.contains("rename"), "error must name the failed operation: {s}");
    assert!(s.contains(&old.claims.display().to_string()));
}

#[test]
fn has_content_detects_empty_vs_nonempty() {
    let dir = tmp();
    let empty = dir.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert!(!has_content(&empty));
    fs::write(empty.join("x"), "y").unwrap();
    assert!(has_content(&empty));
    assert!(!has_content(&dir.path().join("absent")));
}
