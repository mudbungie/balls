//! Unit coverage for `store_legacy`'s stealth branches. Under XDG
//! production code the legacy `.balls/local/tasks_dir` marker is
//! unreachable (stealth lives in `clone.json`), but `Store::discover`
//! must still resolve a hand-planted legacy stealth layout — `bl
//! migrate` reads it.

use super::*;
use std::fs;
use tempfile::TempDir;

/// A hand-built legacy stealth clone: a git repo with a
/// `.balls/config.json` marker and a `.balls/local/tasks_dir` pointer
/// at an external tasks directory. Returns (holding tempdir, repo
/// root, external tasks dir).
fn legacy_stealth_repo() -> (TempDir, PathBuf, PathBuf) {
    let td = TempDir::new().unwrap();
    let root = td.path().to_path_buf();
    crate::git_test_support::init_repo(&root);
    fs::create_dir_all(root.join(".balls/local")).unwrap();
    fs::create_dir_all(root.join(".balls/plugins")).unwrap();
    crate::config::Config::default()
        .save(&root.join(".balls/config.json"))
        .unwrap();
    let ext = td.path().join("ext-tasks");
    fs::create_dir_all(&ext).unwrap();
    fs::write(
        root.join(".balls/local/tasks_dir"),
        ext.to_string_lossy().as_bytes(),
    )
    .unwrap();
    (td, root, ext)
}

#[test]
fn discover_legacy_stealth_via_tasks_dir_marker() {
    let (_td, root, ext) = legacy_stealth_repo();
    let store = discover(&root).expect("legacy stealth discover");
    assert_eq!(store.tasks_dir(), ext);
    assert!(store.stealth);
    assert_eq!(store.layout, Layout::Legacy);
}

#[test]
fn discover_no_git_legacy_stealth() {
    // `discover_no_git` resolves a legacy stealth clone outside any
    // git repo. Hand-plant the layout in a plain tempdir.
    let td = TempDir::new().unwrap();
    let root = td.path().to_path_buf();
    fs::create_dir_all(root.join(".balls/local")).unwrap();
    crate::config::Config::default()
        .save(&root.join(".balls/config.json"))
        .unwrap();
    let ext = td.path().join("ext-tasks");
    fs::create_dir_all(&ext).unwrap();
    fs::write(
        root.join(".balls/local/tasks_dir"),
        ext.to_string_lossy().as_bytes(),
    )
    .unwrap();

    let store = discover_no_git(&root).expect("no-git stealth discover");
    assert!(store.no_git);
    assert!(store.stealth);
    assert_eq!(store.tasks_dir(), ext);
}

#[test]
fn discover_no_git_without_stealth_marker_errors() {
    // No `.balls/local/tasks_dir` pointer: non-stealth needs git for
    // origin, so the no-git path can't resolve.
    let td = TempDir::new().unwrap();
    let root = td.path().to_path_buf();
    fs::create_dir_all(root.join(".balls")).unwrap();
    crate::config::Config::default()
        .save(&root.join(".balls/config.json"))
        .unwrap();
    let err = discover_no_git(&root).err().expect("must error");
    assert!(format!("{err:?}").contains("no_git") || format!("{err}").contains("not initialized"));
}

#[test]
fn discover_errors_when_state_worktree_lacks_tasks_dir() {
    // Manually scaffold a legacy non-stealth clone whose state-repo's
    // `.balls/tasks` was removed — discover surfaces the breakage.
    let td = TempDir::new().unwrap();
    let root = td.path().to_path_buf();
    crate::git_test_support::init_repo(&root);
    fs::create_dir_all(root.join(".balls/plugins")).unwrap();
    crate::config::Config::default()
        .save(&root.join(".balls/config.json"))
        .unwrap();
    // Materialize state-repo via ensure(), then strip its tasks dir.
    let cfg = crate::config::Config::load(&root.join(".balls/config.json")).unwrap();
    let addr = crate::tracker_address::resolve(&root, &cfg);
    let sd = crate::state_repo::ensure(&root, &addr).unwrap();
    fs::remove_dir_all(sd.join(".balls/tasks")).unwrap();
    let err = discover(&root).err().expect("must error");
    let s = format!("{err}");
    assert!(s.contains("task state is") || s.contains("missing"), "got: {s}");
}
