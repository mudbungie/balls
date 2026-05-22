//! `bl init --bare` — first-class bare central-hub bootstrap. The
//! tool-mechanized form of README *Bootstrapping a bare hub from
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
    let _id = create_task(dev.path(), "hub task");
    push(dev.path());
    (remote, "hub task".to_string())
}

#[test]
fn bare_hub_bootstrap_reconstructs_the_loose_store() {
    let (remote, title) = published_remote();
    let run = tmp();
    let hub = run.path().join("proj-hub");

    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&hub)
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized bare balls hub"));

    // Bare gitdir at <hub>/.git, plus the loose store reconstructed.
    assert_eq!(
        git(&hub, &["rev-parse", "--is-bare-repository"]).trim(),
        "true"
    );
    assert!(hub.join(".balls/config.json").exists());
    assert!(hub.join(".balls/tasks").is_symlink());
    assert!(hub.join(".balls/state-repo").exists());
    assert!(hub.join(".balls/local/claims").exists());
    assert!(hub.join(".balls/local/lock").exists());

    // The hub serves the project's tasks from the bare root (bl-8cf7).
    bl(&hub)
        .args(["list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains(title));
}

#[test]
fn bare_hub_bootstrap_is_idempotent() {
    let (remote, title) = published_remote();
    let run = tmp();
    let hub = run.path().join("proj-hub");

    for _ in 0..2 {
        bl(run.path())
            .args(["init", "--bare"])
            .arg(remote.path())
            .arg(&hub)
            .assert()
            .success();
    }
    // Second run reused the bare gitdir and the materialized config; the
    // store still resolves and lists the task.
    bl(&hub)
        .args(["list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains(title));
}

#[test]
fn bare_hub_rejects_stealth_or_tasks_dir_combo() {
    let run = tmp();
    let hub = run.path().join("proj-hub");
    bl(run.path())
        .args(["init", "--bare"])
        .arg("/some/source")
        .arg(&hub)
        .arg("--stealth")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--bare cannot be combined with --stealth or --tasks-dir",
        ));
}

#[test]
fn bare_hub_source_without_balls_errors_clearly() {
    // A remote whose `main` was never `bl init`ed: no config.json to
    // materialize from.
    let remote = new_bare_remote();
    let dev = clone_from_remote(remote.path(), "bob");
    std::fs::write(dev.path().join("README"), "hi\n").unwrap();
    git(dev.path(), &["add", "-A"]);
    git(dev.path(), &["commit", "-qm", "init", "--no-verify"]);
    git(dev.path(), &["push", "-q", "origin", "main"]);

    let run = tmp();
    let hub = run.path().join("proj-hub");
    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&hub)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no .balls/config.json"));
}

#[test]
fn bare_hub_refuses_to_clobber_a_non_bare_gitdir() {
    let (remote, _title) = published_remote();
    let run = tmp();
    let hub = run.path().join("proj-hub");
    // Pre-existing *non-bare* .git at the hub path: must be refused, not
    // overwritten (non-destructive, like working-tree `bl init`).
    std::fs::create_dir_all(&hub).unwrap();
    git(&hub, &["init", "-q", "-b", "main"]);
    assert!(hub.join(".git").is_dir());

    bl(run.path())
        .args(["init", "--bare"])
        .arg(remote.path())
        .arg(&hub)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a bare repo"));
}
