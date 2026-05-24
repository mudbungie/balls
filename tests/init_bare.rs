//! `bl init --bare` — first-class bare-clone bootstrap. The
//! tool-mechanized form of README *Bootstrapping a bare clone from
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
    let _id = create_task(dev.path(), "clone task");
    push(dev.path());
    (remote, "clone task".to_string())
}

#[test]
fn bare_clone_bootstrap_reconstructs_the_loose_store() {
    let (remote, title) = published_remote();
    let run = tmp();
    let clone = run.path().join("proj-clone");

    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&clone)
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized bare balls clone"));

    // Bare gitdir at <clone>/.git, plus the loose store reconstructed.
    assert_eq!(
        git(&clone, &["rev-parse", "--is-bare-repository"]).trim(),
        "true"
    );
    assert!(clone.join(".balls/config.json").exists());
    assert!(clone.join(".balls/tasks").is_symlink());
    assert!(clone.join(".balls/state-repo").exists());
    assert!(clone.join(".balls/local/claims").exists());
    assert!(clone.join(".balls/local/lock").exists());

    // The clone serves the project's tasks from the bare root (bl-8cf7).
    bl(&clone)
        .args(["list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains(title));
}

#[test]
fn bare_clone_bootstrap_is_idempotent() {
    let (remote, title) = published_remote();
    let run = tmp();
    let clone = run.path().join("proj-clone");

    for _ in 0..2 {
        bl(run.path())
            .args(["init", "--bare"])
            .arg(remote.path())
            .arg(&clone)
            .assert()
            .success();
    }
    // Second run reused the bare gitdir and the materialized config; the
    // store still resolves and lists the task.
    bl(&clone)
        .args(["list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains(title));
}

#[test]
fn bare_clone_rejects_stealth_or_tasks_dir_combo() {
    let run = tmp();
    let clone = run.path().join("proj-clone");
    bl(run.path())
        .args(["init", "--bare"])
        .arg("/some/source")
        .arg(&clone)
        .arg("--stealth")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--bare cannot be combined with --stealth or --tasks-dir",
        ));
}

#[test]
fn bare_clone_source_without_balls_errors_clearly() {
    // A remote whose `main` was never `bl init`ed: no config.json to
    // materialize from.
    let remote = new_bare_remote();
    let dev = clone_from_remote(remote.path(), "bob");
    std::fs::write(dev.path().join("README"), "hi\n").unwrap();
    git(dev.path(), &["add", "-A"]);
    git(dev.path(), &["commit", "-qm", "init", "--no-verify"]);
    git(dev.path(), &["push", "-q", "origin", "main"]);

    let run = tmp();
    let clone = run.path().join("proj-clone");
    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&clone)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no .balls/config.json"));
}

#[test]
fn bare_clone_refuses_to_clobber_a_non_bare_gitdir() {
    let (remote, _title) = published_remote();
    let run = tmp();
    let clone = run.path().join("proj-clone");
    // Pre-existing *non-bare* .git at the clone path: must be refused,
    // not overwritten (non-destructive, like working-tree `bl init`).
    std::fs::create_dir_all(&clone).unwrap();
    git(&clone, &["init", "-q", "-b", "main"]);
    assert!(clone.join(".git").is_dir());

    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&clone)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a bare repo"));
}
