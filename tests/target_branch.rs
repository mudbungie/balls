//! bl-0c99 — `target_branch` makes the integration branch explicit.
//!
//! Conformance: unset `target_branch` is byte-identical to before the
//! field — `bl review` squashes into whatever is checked out at the
//! root and the key never serializes. The positive case: with
//! `target_branch=develop` and `main` checked out at the root, the
//! squash lands on `develop`, `main` is untouched, and `bl sync`
//! pushes `develop` alongside the state branch.

mod common;

use common::*;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

fn sha(repo: &Path, refname: &str) -> String {
    git(repo, &["rev-parse", refname]).trim().to_string()
}

fn subject(repo: &Path, refname: &str) -> String {
    git(repo, &["log", "-1", "--format=%s", refname])
}

/// `target_branch=develop` with `main` checked out: the squash lands
/// on `develop`, `main` and the root work tree are untouched, and the
/// delivery hint points at the new `develop` commit.
#[test]
fn review_squashes_into_configured_target_branch_not_checkout() {
    let repo = new_repo();
    init_in(repo.path());
    git(repo.path(), &["branch", "develop"]);

    let main_before = sha(repo.path(), "main");
    let develop_before = sha(repo.path(), "develop");
    assert_eq!(main_before, develop_before, "develop forks from main");

    // SPEC §6.7 (post-XDG): repo-level `target_branch` is retired; the
    // resolution chain is `task.target_branch ?? HEAD@root`. Set the
    // per-task target via the test-helper default so `bl create`
    // forwards `--target-branch develop` on this task.
    set_default_target_branch(Some("develop".into()));
    let id = create_task(repo.path(), "feature");
    set_default_target_branch(None);
    bl(repo.path()).args(["claim", &id]).assert().success();
    let wt = worktree_path(repo.path(), &id);
    fs::write(wt.join("feature.txt"), "work").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();

    assert_eq!(sha(repo.path(), "main"), main_before, "main untouched");
    let develop_after = sha(repo.path(), "develop");
    assert_ne!(develop_after, develop_before, "develop advanced");
    assert!(
        subject(repo.path(), "develop").contains(&format!("[{id}]")),
        "squash on develop must carry the delivery tag"
    );
    assert!(
        !repo.path().join("feature.txt").exists(),
        "root work tree (main checked out) must be untouched"
    );
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "review");
    assert_eq!(
        j["delivered_in"].as_str().unwrap(),
        develop_after,
        "delivered_in points at the develop squash"
    );

    bl(repo.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();
    assert!(!discover_tasks_dir(repo.path()).join(format!("{id}.json")).exists());
}

/// Unset `target_branch` is byte-identical to before the field: the
/// key never serializes and the squash lands on the checkout.
#[test]
fn unset_target_branch_squashes_into_checkout() {
    let repo = new_repo();
    init_in(repo.path());

    let cfg = fs::read_to_string(config_path(repo.path())).unwrap();
    assert!(
        !cfg.contains("target_branch"),
        "unset target_branch must not serialize: {cfg}"
    );

    let main_before = sha(repo.path(), "main");
    let id = create_task(repo.path(), "feature");
    bl(repo.path()).args(["claim", &id]).assert().success();
    let wt = worktree_path(repo.path(), &id);
    fs::write(wt.join("feature.txt"), "work").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();

    assert_ne!(sha(repo.path(), "main"), main_before, "main advanced");
    assert!(
        repo.path().join("feature.txt").exists(),
        "default squash lands on the checked-out branch"
    );
}

/// `bl sync` pushes the configured `target_branch` to the code remote
/// alongside the state branch; `main` is not advanced by the sync.
#[test]
fn sync_pushes_configured_target_branch() {
    let code = new_bare_remote();
    let alice = clone_from_remote(code.path(), "alice");
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["branch", "develop"]);
    git(alice.path(), &["push", "origin", "main"]);
    git(alice.path(), &["push", "origin", "develop"]);

    let main_remote_before = sha(code.path(), "main");
    // Per-task target replaces the retired repo-level `target_branch`
    // field (SPEC §6.7); the helper-default forwards it to `bl create`.
    set_default_target_branch(Some("develop".into()));
    let id = create_task(alice.path(), "feature");
    set_default_target_branch(None);
    bl(alice.path()).args(["claim", &id]).assert().success();
    let wt = worktree_path(alice.path(), &id);
    fs::write(wt.join("feature.txt"), "work").unwrap();
    bl(alice.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();
    bl(alice.path()).arg("sync").assert().success();

    assert!(
        git_ok(
            code.path(),
            &["rev-parse", "--verify", "--quiet", "refs/heads/balls/tasks"],
        ),
        "state branch must be pushed alongside"
    );
    assert_eq!(
        sha(alice.path(), "develop"),
        sha(code.path(), "develop"),
        "bl sync pushes the configured target_branch to the code remote"
    );
    assert!(
        subject(code.path(), "develop").contains(&format!("[{id}]")),
        "pushed develop carries the delivery tag"
    );
    assert_eq!(
        sha(code.path(), "main"),
        main_remote_before,
        "sync must not advance main"
    );
}

/// Run `bl create --target-branch B TITLE`, returning the new id.
fn create_with_target(repo: &Path, title: &str, branch: &str) -> String {
    let out = bl(repo)
        .args(["create", title, "--target-branch", branch])
        .output()
        .expect("bl create");
    assert!(out.status.success(), "bl create --target-branch failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// bl-d4b0 acceptance: `bl create --target-branch` writes the field;
/// `bl review` squashes into it, beating BOTH the repo-level
/// `target_branch` (develop) AND the current-branch fallback (main);
/// `bl show` surfaces it; the task closes cleanly.
#[test]
fn per_task_target_branch_overrides_config_and_checkout() {
    let repo = new_repo();
    seed_config(repo.path(), &[("target_branch", "develop")]);
    init_in(repo.path());
    git(repo.path(), &["branch", "develop"]);
    git(repo.path(), &["branch", "release"]);

    let main_before = sha(repo.path(), "main");
    let develop_before = sha(repo.path(), "develop");
    let release_before = sha(repo.path(), "release");

    let id = create_with_target(repo.path(), "hotfix", "release");
    let j = read_task_json(repo.path(), &id);
    assert_eq!(
        j["target_branch"].as_str(),
        Some("release"),
        "bl create --target-branch must write the field"
    );

    bl(repo.path())
        .args(["show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("target:"))
        .stdout(predicate::str::contains("release"));
    let jshow: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(
        &bl(repo.path())
            .args(["show", &id, "--json"])
            .output()
            .unwrap()
            .stdout,
    ))
    .unwrap();
    assert_eq!(jshow["task"]["target_branch"], "release");

    bl(repo.path()).args(["claim", &id]).assert().success();
    let wt = worktree_path(repo.path(), &id);
    fs::write(wt.join("hotfix.txt"), "fix").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "ship hotfix"])
        .assert()
        .success();

    assert_eq!(sha(repo.path(), "main"), main_before, "main untouched");
    assert_eq!(
        sha(repo.path(), "develop"),
        develop_before,
        "repo-level develop must be ignored when the task overrides it"
    );
    let release_after = sha(repo.path(), "release");
    assert_ne!(release_after, release_before, "release advanced");
    assert!(
        subject(repo.path(), "release").contains(&format!("[{id}]")),
        "squash must land on the per-task target"
    );
    assert_eq!(
        read_task_json(repo.path(), &id)["delivered_in"].as_str(),
        Some(release_after.as_str())
    );

    bl(repo.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();
    assert!(!discover_tasks_dir(repo.path()).join(format!("{id}.json")).exists());
}

/// A task created without `--target-branch` must not serialize the
/// key: existing task files stay byte-identical and an older `bl`
/// reading the file is unaffected.
#[test]
fn unset_per_task_target_branch_is_not_serialized() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "plain");
    let raw = fs::read_to_string(
        discover_tasks_dir(repo.path()).join(format!("{id}.json")),
    )
    .unwrap();
    assert!(
        !raw.contains("target_branch"),
        "unset per-task target_branch must not serialize: {raw}"
    );
}
