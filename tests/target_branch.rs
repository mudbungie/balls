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
use std::fs;
use std::path::Path;

/// Write a minimal valid `.balls/config.json` before `bl init` so the
/// repo is targeting `target_branch` from its first lifecycle command
/// (mirrors `tests/state_remote.rs::seed_config`).
fn seed_config(repo: &Path, target_branch: Option<&str>) {
    let balls = repo.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    let tb = match target_branch {
        Some(b) => format!(r#","target_branch":"{b}""#),
        None => String::new(),
    };
    fs::write(
        balls.join("config.json"),
        format!(
            r#"{{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"{tb}}}"#
        ),
    )
    .unwrap();
}

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
    seed_config(repo.path(), Some("develop"));
    init_in(repo.path());
    git(repo.path(), &["branch", "develop"]);

    let main_before = sha(repo.path(), "main");
    let develop_before = sha(repo.path(), "develop");
    assert_eq!(main_before, develop_before, "develop forks from main");

    let id = create_task(repo.path(), "feature");
    bl(repo.path()).args(["claim", &id]).assert().success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
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
    assert!(!repo.path().join(format!(".balls/tasks/{id}.json")).exists());
}

/// Unset `target_branch` is byte-identical to before the field: the
/// key never serializes and the squash lands on the checkout.
#[test]
fn unset_target_branch_squashes_into_checkout() {
    let repo = new_repo();
    init_in(repo.path());

    let cfg = fs::read_to_string(repo.path().join(".balls/config.json")).unwrap();
    assert!(
        !cfg.contains("target_branch"),
        "unset target_branch must not serialize: {cfg}"
    );

    let main_before = sha(repo.path(), "main");
    let id = create_task(repo.path(), "feature");
    bl(repo.path()).args(["claim", &id]).assert().success();
    let wt = repo.path().join(".balls-worktrees").join(&id);
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
    seed_config(alice.path(), Some("develop"));
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["branch", "develop"]);
    git(alice.path(), &["push", "origin", "main"]);
    git(alice.path(), &["push", "origin", "develop"]);

    let main_remote_before = sha(code.path(), "main");
    let id = create_task(alice.path(), "feature");
    bl(alice.path()).args(["claim", &id]).assert().success();
    let wt = alice.path().join(".balls-worktrees").join(&id);
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
