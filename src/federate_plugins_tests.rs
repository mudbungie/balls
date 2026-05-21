//! Plugin stash/migrate unit tests for `federate` (bl-82a4). Split
//! from `federate_tests.rs` to keep both under the 300-line cap.

use super::*;
use std::os::unix::fs::symlink;
use tempfile::TempDir;

#[test]
fn stash_plugins_renames_a_real_dir_aside() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let project_plugins = root.join(".balls/plugins");
    fs::create_dir_all(&project_plugins).unwrap();
    fs::write(project_plugins.join("a.json"), "x").unwrap();
    let stash = stash_plugins(root).unwrap().expect("real dir ⇒ stashed");
    assert!(!project_plugins.exists(), "original moved aside");
    assert!(stash.join("a.json").exists());
}

#[test]
fn stash_plugins_clears_a_leftover_stash_from_a_crashed_run() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let project_plugins = root.join(".balls/plugins");
    fs::create_dir_all(&project_plugins).unwrap();
    fs::write(project_plugins.join("fresh.json"), "new").unwrap();
    // A crashed prior federate left a stale stash dir behind.
    let stale = root.join(PLUGINS_STASH);
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("stale.json"), "old").unwrap();

    let stash = stash_plugins(root).unwrap().expect("real dir ⇒ stashed");
    assert!(stash.join("fresh.json").exists());
    assert!(
        !stash.join("stale.json").exists(),
        "stale stash content must be cleared, not merged"
    );
}

#[test]
fn stash_plugins_returns_none_for_symlink_or_absent() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".balls")).unwrap();
    // Absent: nothing to stash.
    assert!(stash_plugins(root).unwrap().is_none());
    // Symlink: bl-1098 already federated it; nothing to stash.
    symlink("elsewhere", root.join(".balls/plugins")).unwrap();
    assert!(stash_plugins(root).unwrap().is_none());
}

#[test]
fn migrate_stashed_plugins_moves_unique_entries_hub_wins() {
    let dir = TempDir::new().unwrap();
    let stash = dir.path().join("stash");
    let hub_plugins = dir.path().join("hub_plugins");
    fs::create_dir_all(&stash).unwrap();
    fs::create_dir_all(&hub_plugins).unwrap();
    fs::write(stash.join("a.json"), "project-a").unwrap();
    fs::write(stash.join("b.json"), "project-b").unwrap();
    // A nested directory exercises move_entry's dir branch and
    // copy_dir_recursive's nested-dir recursion.
    fs::create_dir_all(stash.join("nested/deep")).unwrap();
    fs::write(stash.join("nested/deep/c.json"), "project-c").unwrap();
    fs::write(hub_plugins.join("a.json"), "hub-a").unwrap();

    migrate_stashed_plugins(&stash, &hub_plugins).unwrap();

    assert_eq!(fs::read_to_string(hub_plugins.join("a.json")).unwrap(), "hub-a");
    assert_eq!(fs::read_to_string(hub_plugins.join("b.json")).unwrap(), "project-b");
    assert_eq!(
        fs::read_to_string(hub_plugins.join("nested/deep/c.json")).unwrap(),
        "project-c"
    );
}
