//! Unit coverage for `commit_init`: the clone-checkout commit at
//! `bl init`. Non-stealth commits `.gitignore` and the repo-owned
//! `.balls/config.json`; the state checkout's symlinks are gitignored
//! runtime state. Stealth has no state checkout, so it additionally
//! owns a real `.balls/plugins/` with a `.gitkeep`.

use super::*;
use crate::git_test_support::{git_stdout, init_repo};
use tempfile::TempDir;

/// Scaffold the `.balls/` layout `commit_init` expects: a real plugins
/// directory and a repo `config.json`.
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

#[test]
fn store_init_legacy_rejects_relative_tasks_dir() {
    // `Store::init` (legacy) is now only called from in-source tests
    // and the migration fixtures (production routes through
    // `Store::init_xdg`). The argument-validation guard for a
    // relative --tasks-dir still lives in the legacy entry point and
    // would otherwise go uncovered.
    let td = TempDir::new().unwrap();
    let res = crate::store::Store::init(td.path(), true, Some("rel/path".into()));
    let err = res.err().expect("relative --tasks-dir must error");
    assert!(format!("{err}").contains("must be an absolute path"), "got: {err}");
}

#[test]
fn store_init_legacy_stealth_without_tasks_dir_uses_sha_path() {
    // Legacy `Store::init` with --stealth and no --tasks-dir derives
    // the tasks dir from sha1(canon(root)) under HOME (the on-disk
    // contract documented on `stealth_tasks_dir`). Production code
    // (post-XDG) routes through `init_xdg`, so this path is unit-
    // tested directly to keep the legacy code under coverage.
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    let store = crate::store::Store::init(td.path(), true, None).unwrap();
    assert!(store.stealth);
    let canon = fs::canonicalize(td.path()).unwrap();
    let expected = crate::store_paths::stealth_tasks_dir(&canon);
    assert_eq!(store.tasks_dir(), expected);
}

#[test]
fn store_init_legacy_no_git_without_tasks_dir_errors() {
    // Outside a git repo, legacy `Store::init` requires --tasks-dir to
    // pick the stealth branch. With neither --stealth nor --tasks-dir,
    // the `git_root` lookup errors and bubbles up.
    let td = TempDir::new().unwrap();
    let res = crate::store::Store::init(td.path(), false, None);
    let err = res.err().expect("no-git non-stealth init must error");
    assert!(matches!(err, crate::error::BallError::NotARepo), "got: {err:?}");
}
