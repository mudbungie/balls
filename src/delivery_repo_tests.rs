//! [`Project`] tests on throwaway project repos — every worktree git act and
//! its idempotent re-run (materialize / release / discard / integration /
//! is_git_repo) plus the task-path and repo-identity reads. The direct squash
//! itself lives in the sibling `deliver_tests` module.

use super::*;
use crate::delivery::Repo;
use std::fs;
use tempfile::TempDir;

/// A throwaway project repo on `main` with one seed commit. Returns the tempdir
/// (kept alive), its root, and a [`Project`]. Shared with the sibling
/// `gate_tests` module.
pub fn project() -> (TempDir, PathBuf, Project) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("proj");
    fs::create_dir(&root).unwrap();
    let g = |args: &[&str]| Project::run(&root, args).unwrap();
    g(&["init", "-q", "-b", "main"]);
    g(&["config", "user.name", "test"]);
    g(&["config", "user.email", "test@example.com"]);
    fs::write(root.join("seed.txt"), "seed\n").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-q", "-m", "seed"]);
    (tmp, root.clone(), Project::at(&root))
}

/// `main`'s tip subject — the delivery assertion surface.
pub fn tip(root: &Path) -> String {
    Project::run(root, &["log", "-1", "--format=%s", "main"]).unwrap().trim().to_string()
}

#[test]
fn materialize_creates_then_is_idempotent_then_reattaches_an_existing_branch() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");

    // Branch absent → create work/<id> off HEAD.
    p.materialize(&wt, "work/bl-x").unwrap();
    assert!(wt.join("seed.txt").exists());
    assert!(p.branch_exists("work/bl-x").unwrap());

    // Path present → no-op (no second `worktree add`, which would fail).
    p.materialize(&wt, "work/bl-x").unwrap();

    // Worktree gone but branch kept → reattach the existing branch.
    p.release(&wt).unwrap();
    assert!(!wt.exists() && p.branch_exists("work/bl-x").unwrap());
    p.materialize(&wt, "work/bl-x").unwrap();
    assert!(wt.join("seed.txt").exists());

    let _ = root;
}

#[test]
fn materialize_recovers_a_deleted_dir_with_a_stale_registration() {
    // The ordinary form of "absent" (bl-b404): the dir was rm -rf'd, not
    // `worktree remove`d, so git still holds a registration. A bare
    // `worktree add` aborts with "missing but already registered worktree";
    // materialize must prune the stale registration and re-materialize.
    let (tmp, _root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::remove_dir_all(&wt).unwrap(); // crash / tmp cleaner / human

    p.materialize(&wt, "work/bl-x").unwrap();
    assert!(wt.join("seed.txt").exists());
}

#[test]
fn release_removes_a_present_worktree_and_no_ops_when_absent() {
    let (tmp, _root, p) = project();
    let wt = tmp.path().join("wt");
    p.release(&wt).unwrap(); // absent → no-op
    p.materialize(&wt, "work/bl-x").unwrap();
    p.release(&wt).unwrap();
    assert!(!wt.exists());
}

#[test]
fn discard_removes_the_worktree_and_deletes_the_branch() {
    let (tmp, _root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    p.discard(&wt, "work/bl-x").unwrap();
    assert!(!wt.exists());
    assert!(!p.branch_exists("work/bl-x").unwrap());
    // Idempotent: both worktree and branch already gone.
    p.discard(&wt, "work/bl-x").unwrap();
}

#[test]
fn integration_is_the_project_head_branch() {
    let (_tmp, _root, p) = project();
    assert_eq!(p.integration().unwrap(), "main");
}

#[test]
fn is_git_repo_holds_for_a_worktree_and_a_bare_repo_but_not_a_plain_dir() {
    // The bl-4a88 precondition predicate, read by EXIT CODE: a normal work tree
    // is a repo, and so is a BARE one (the common balls deployment — delivery
    // works against it, so the gate must NOT reject it), while a plain dir is
    // not — the only case `claim`/`close` should abort on.
    let (_tmp, _root, p) = project();
    assert!(p.is_git_repo().unwrap());

    let bare = TempDir::new().unwrap();
    Project::run(bare.path(), &["init", "-q", "--bare", "-b", "main"]).unwrap();
    assert!(Project::at(bare.path()).is_git_repo().unwrap());

    let plain = TempDir::new().unwrap();
    assert!(!Project::at(plain.path()).is_git_repo().unwrap());
}

#[test]
fn changed_task_paths_lists_the_ops_touched_task_file() {
    let (tmp, root, _p) = project();
    fs::create_dir(root.join("tasks")).unwrap();
    fs::write(root.join("tasks/bl-9f9f.md"), "x\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "add task"]).unwrap();
    fs::remove_file(root.join("tasks/bl-9f9f.md")).unwrap(); // the close diff

    assert_eq!(changed_task_paths(&root).unwrap(), ["tasks/bl-9f9f.md"]);
    let _ = tmp;
}

#[test]
fn a_git_failure_surfaces_with_stderr() {
    let outside = TempDir::new().unwrap(); // not a git repo
    let err = changed_task_paths(outside.path()).unwrap_err();
    assert!(err.to_string().starts_with("git diff"));
}

#[test]
fn root_commit_is_the_seed_root_or_none_off_a_non_repo() {
    // bl-1ce7: the canonical, remote-free repo identity. On a one-commit repo
    // the root IS that seed commit; off a non-repo dir it is None (a ball
    // created there records nothing — back-compat).
    let (_tmp, root, p) = project();
    let r = p.root_commit().expect("a committed repo has a root");
    let seed = Project::run(&root, &["rev-parse", "HEAD"]).unwrap().trim().to_string();
    assert_eq!(r, seed, "the sole commit is the root");
    // A single-root repo's SET is exactly that seed (the canonical stamp = line 1).
    assert_eq!(p.root_commits(), vec![seed]);
    let outside = TempDir::new().unwrap(); // not a git repo
    assert!(Project::at(outside.path()).root_commit().is_none());
    assert!(Project::at(outside.path()).root_commits().is_empty());
}
