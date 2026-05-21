//! Unit coverage for `commit_init`: the workspace-checkout commit at
//! `bl init`. Non-stealth commits `.gitignore` and the workspace-owned
//! `.balls/config.json`; the state checkout's symlinks are gitignored
//! runtime state. Stealth has no state checkout, so it additionally
//! owns a real `.balls/plugins/` with a `.gitkeep`.

use super::*;
use crate::git_test_support::{git_stdout, init_repo};
use tempfile::TempDir;

/// Scaffold the `.balls/` layout `commit_init` expects: a real plugins
/// directory and a workspace `config.json`.
fn scaffold(root: &Path) {
    fs::create_dir_all(root.join(".balls/plugins")).unwrap();
    Config::default().save(&root.join(".balls/config.json")).unwrap();
}

fn tracked(root: &Path) -> String {
    git_stdout(root, &["ls-files"])
}

fn last_subject(root: &Path) -> String {
    git_stdout(root, &["log", "-1", "--format=%s"])
}

#[test]
fn non_stealth_commits_config_and_gitignore() {
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    scaffold(td.path());

    commit_init(td.path(), false, false).unwrap();

    let files = tracked(td.path());
    assert!(files.contains(".balls/config.json"), "{files}");
    assert!(files.contains(".gitignore"), "{files}");
    assert!(
        !files.contains(".gitkeep"),
        "the state checkout's plugins dir is a gitignored symlink: {files}"
    );
    assert_eq!(last_subject(td.path()), "balls: initialize");
}

#[test]
fn stealth_seeds_and_stages_the_plugins_gitkeep() {
    // Stealth has no state checkout, so `.balls/plugins/` stays a real
    // committed directory with a placeholder.
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    scaffold(td.path());

    commit_init(td.path(), true, false).unwrap();

    assert!(td.path().join(".balls/plugins/.gitkeep").exists());
    assert!(tracked(td.path()).contains(".balls/plugins/.gitkeep"));
}

#[test]
fn reinitialize_uses_reinitialize_subject() {
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    scaffold(td.path());

    commit_init(td.path(), false, true).unwrap();

    assert_eq!(last_subject(td.path()), "balls: reinitialize");
}

#[test]
fn stealth_leaves_an_existing_gitkeep_intact() {
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    scaffold(td.path());
    let keep = td.path().join(".balls/plugins/.gitkeep");
    fs::write(&keep, "sentinel").unwrap();

    commit_init(td.path(), true, false).unwrap();

    assert_eq!(fs::read_to_string(&keep).unwrap(), "sentinel");
}
