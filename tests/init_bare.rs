//! `bl init --bare` — first-class bare-workspace bootstrap. The
//! tool-mechanized form of README *Bootstrapping a bare workspace from
//! scratch* steps 2–3 (bl-9e8a). Idempotent and non-destructive,
//! exactly like the working-tree `bl init`.

mod common;

use common::*;
use predicates::prelude::*;

/// Stand up a published project on a bare remote: a working clone runs
/// `bl init` (creating + pushing `balls/tasks` and committing
/// `config.json` to main), adds a task, and pushes everything. Returns
/// the remote plus the created task's title.
fn published_remote() -> (Repo, String) {
    let remote = new_bare_remote();
    let dev = clone_from_remote(remote.path(), "alice");
    bl(dev.path()).arg("init").assert().success();
    push(dev.path());
    let _id = create_task(dev.path(), "workspace task");
    push(dev.path());
    (remote, "workspace task".to_string())
}

#[test]
fn bare_workspace_bootstrap_reconstructs_the_loose_store() {
    let (remote, title) = published_remote();
    let run = tmp();
    let workspace = run.path().join("proj-workspace");

    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized bare balls workspace"));

    // Bare gitdir at <workspace>/.git, plus the loose store reconstructed.
    assert_eq!(
        git(&workspace, &["rev-parse", "--is-bare-repository"]).trim(),
        "true"
    );
    assert!(workspace.join(".balls/config.json").exists());
    assert!(workspace.join(".balls/tasks").is_symlink());
    assert!(workspace.join(".balls/state-repo").exists());
    assert!(workspace.join(".balls/local/claims").exists());
    assert!(workspace.join(".balls/local/lock").exists());

    // The workspace serves the project's tasks from the bare root (bl-8cf7).
    bl(&workspace)
        .args(["list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains(title));
}

#[test]
fn bare_workspace_bootstrap_is_idempotent() {
    let (remote, title) = published_remote();
    let run = tmp();
    let workspace = run.path().join("proj-workspace");

    for _ in 0..2 {
        bl(run.path())
            .args(["init", "--bare"])
            .arg(remote.path())
            .arg(&workspace)
            .assert()
            .success();
    }
    // Second run reused the bare gitdir and the materialized config; the
    // store still resolves and lists the task.
    bl(&workspace)
        .args(["list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains(title));
}

#[test]
fn bare_workspace_rejects_stealth_or_tasks_dir_combo() {
    let run = tmp();
    let workspace = run.path().join("proj-workspace");
    bl(run.path())
        .args(["init", "--bare"])
        .arg("/some/source")
        .arg(&workspace)
        .arg("--stealth")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--bare cannot be combined with --stealth or --tasks-dir",
        ));
}

#[test]
fn bare_workspace_source_without_balls_errors_clearly() {
    // A remote whose `main` was never `bl init`ed: no config.json to
    // materialize from.
    let remote = new_bare_remote();
    let dev = clone_from_remote(remote.path(), "bob");
    std::fs::write(dev.path().join("README"), "hi\n").unwrap();
    git(dev.path(), &["add", "-A"]);
    git(dev.path(), &["commit", "-qm", "init", "--no-verify"]);
    git(dev.path(), &["push", "-q", "origin", "main"]);

    let run = tmp();
    let workspace = run.path().join("proj-workspace");
    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&workspace)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no .balls/config.json"));
}

#[test]
fn bare_workspace_refuses_to_clobber_a_non_bare_gitdir() {
    let (remote, _title) = published_remote();
    let run = tmp();
    let workspace = run.path().join("proj-workspace");
    // Pre-existing *non-bare* .git at the workspace path: must be refused,
    // not overwritten (non-destructive, like working-tree `bl init`).
    std::fs::create_dir_all(&workspace).unwrap();
    git(&workspace, &["init", "-q", "-b", "main"]);
    assert!(workspace.join(".git").is_dir());

    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&workspace)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a bare repo"));
}
