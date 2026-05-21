//! bl-20ad — `bl remaster <url>` onto a hub has no task-carry path
//! (that is the name path's `reconcile`). Flipping a standalone repo
//! that still holds local tasks would strand them, invisible, on the
//! orphaned `.balls/worktree`. The URL flip must refuse rather than
//! lose data silently; `--force` overrides, abandoning the tasks.

mod common;

use common::*;
use predicates::prelude::*;

/// A standalone repo with a local-only task: `bl remaster <url>`
/// aborts before materializing anything, naming the two remedies.
#[test]
fn remaster_url_aborts_when_local_tasks_would_strand() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    let id = create_task(alice.path(), "local only");

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("would be stranded"))
        .stderr(predicate::str::contains("close every task"))
        .stderr(predicate::str::contains("--force"));

    // Aborted before `state_repo::ensure` — nothing materialized, so
    // the repo is clean to retry once the tasks are dealt with.
    assert!(
        !alice.path().join(".balls/state-repo").exists(),
        "abort must not materialize the state-repo"
    );
    // The local task is untouched and fully visible.
    let listing = bl(alice.path()).arg("list").assert().success();
    let stdout = String::from_utf8(listing.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains(&id), "local task must remain visible: {stdout}");
}

/// `--force` federates anyway, warning that the local tasks are
/// abandoned on `.balls/worktree` — gone from the federated store.
#[test]
fn remaster_url_force_federates_and_abandons_local_tasks() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    let id = create_task(alice.path(), "doomed");

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .arg("--force")
        .assert()
        .success()
        .stderr(predicate::str::contains("--force federate abandons"));

    assert!(
        alice.path().join(".balls/state-repo/.git").exists(),
        "--force must complete the federation"
    );
    // The flip adopted the (empty) hub history; the local task is
    // stranded on `.balls/worktree`, no longer in the live store.
    let listing = bl(alice.path()).arg("list").assert().success();
    let stdout = String::from_utf8(listing.get_output().stdout.clone()).unwrap();
    assert!(
        !stdout.contains(&id),
        "--force abandons the local task — gone from the federated store: {stdout}"
    );
}
