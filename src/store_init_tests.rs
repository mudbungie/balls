//! Unit coverage for `commit_init`'s federated-mode decision (bl-4432).
//! The decision must come from `master_url` in config, not from probing
//! for the `.balls/plugins` symlink — so these tests deliberately never
//! create that symlink, only the config that drives the choice.

use super::*;
use crate::git_test_support::{git_run, git_stdout};
use tempfile::TempDir;

fn init_repo(path: &Path) {
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "test@example.com"],
        &["config", "user.name", "test"],
        &["config", "commit.gpgsign", "false"],
        &["commit", "--allow-empty", "-m", "init", "--no-verify"],
    ] {
        git_run(path, args);
    }
}

/// Scaffold the `.balls/` layout `commit_init` expects: a real plugins
/// directory and a config with (or without) a `master_url`.
fn scaffold(root: &Path, master_url: Option<&str>) {
    fs::create_dir_all(root.join(".balls/plugins")).unwrap();
    let cfg = Config { master_url: master_url.map(str::to_string), ..Config::default() };
    cfg.save(&root.join(".balls/config.json")).unwrap();
}

fn tracked(root: &Path) -> String {
    git_stdout(root, &["ls-files"])
}

fn last_subject(root: &Path) -> String {
    git_stdout(root, &["log", "-1", "--format=%s"])
}

#[test]
fn standalone_seeds_and_stages_gitkeep() {
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    scaffold(td.path(), None);

    commit_init(td.path(), false, false).unwrap();

    assert!(td.path().join(".balls/plugins/.gitkeep").exists());
    let files = tracked(td.path());
    assert!(files.contains(".balls/plugins/.gitkeep"), "{files}");
    assert!(files.contains(".balls/config.json"), "{files}");
    assert!(files.contains(".gitignore"), "{files}");
    assert_eq!(last_subject(td.path()), "balls: initialize");
}

#[test]
fn federated_skips_gitkeep_even_without_symlink() {
    // The bl-4432 deliverable: a master_url repo must skip the project
    // `.gitkeep` purely on config — with no `.balls/plugins` symlink
    // present, which is the ordering the old probe silently relied on.
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    scaffold(td.path(), Some("https://hub.example/tasks.git"));
    assert!(!td.path().join(".balls/plugins").is_symlink());

    commit_init(td.path(), false, false).unwrap();

    assert!(
        !td.path().join(".balls/plugins/.gitkeep").exists(),
        "federated mode must not seed a project-owned .gitkeep"
    );
    let files = tracked(td.path());
    assert!(!files.contains(".gitkeep"), "{files}");
    assert!(files.contains(".balls/config.json"), "{files}");
}

#[test]
fn reinitialize_uses_reinitialize_subject() {
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    scaffold(td.path(), None);

    commit_init(td.path(), false, true).unwrap();

    assert_eq!(last_subject(td.path()), "balls: reinitialize");
}

#[test]
fn existing_gitkeep_is_left_intact() {
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    scaffold(td.path(), None);
    let keep = td.path().join(".balls/plugins/.gitkeep");
    fs::write(&keep, "sentinel").unwrap();

    commit_init(td.path(), false, false).unwrap();

    // A pre-existing placeholder is staged but never rewritten.
    assert_eq!(fs::read_to_string(&keep).unwrap(), "sentinel");
    assert!(tracked(td.path()).contains(".balls/plugins/.gitkeep"));
}

#[test]
fn stealth_owns_gitkeep_even_with_master_url_in_config() {
    // Stealth never runs `state_repo::ensure`, so it always owns the
    // placeholder — a stray `master_url` in config must not flip that.
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    scaffold(td.path(), Some("https://hub.example/tasks.git"));

    commit_init(td.path(), true, false).unwrap();

    assert!(td.path().join(".balls/plugins/.gitkeep").exists());
}
