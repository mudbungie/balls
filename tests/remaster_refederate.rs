//! bl-692b — re-federating after a `bl remaster --detach` must adopt
//! the new hub, not silently re-track the detach-era orphan.
//!
//! A `master_url` detach used to leave `.balls/state-repo/` on disk: a
//! balls-owned clone whose `balls/tasks` had been re-rooted to a
//! hub-severed orphan. `state_repo::ensure` keys "warm cache" off that
//! leftover `.git` + `balls/tasks`, so a later re-federation re-pointed
//! `origin` at the new hub but never re-tracked its `balls/tasks` — the
//! repo stayed on its own orphan with no error at flip time. Detach now
//! discards the clone, so re-federation is a clean first-time
//! materialization against whatever hub it names.

mod common;

use common::*;
use std::path::Path;

/// `bl list` stdout for `repo` (asserts the command succeeded).
fn list_output(repo: &Path) -> String {
    let out = bl(repo).arg("list").assert().success();
    String::from_utf8(out.get_output().stdout.clone()).unwrap()
}

fn url(repo: &Repo) -> String {
    repo.path().to_string_lossy().to_string()
}

fn remaster_commit(repo: &Path, hub: &str) {
    bl(repo)
        .arg("remaster")
        .arg(hub)
        .arg("--commit")
        .assert()
        .success();
}

/// federate hub A -> detach -> re-federate to a *different*, already
/// populated hub B. The repo must adopt hub B's `balls/tasks` — see its
/// tasks and `bl sync` onto its history — not its hub-A-era orphan.
#[test]
fn refederate_to_a_different_hub_adopts_it() {
    let hub_a = new_bare_remote();
    let hub_b = new_bare_remote();

    // Bob seeds hub B with a task on its `balls/tasks`.
    let bob = new_repo();
    init_in(bob.path());
    remaster_commit(bob.path(), &url(&hub_b));
    let bob_id = create_task(bob.path(), "bob's hub-B task");
    bl(bob.path()).arg("sync").assert().success();

    // Alice federates to hub A, then detaches back to standalone.
    let alice = new_repo();
    init_in(alice.path());
    remaster_commit(alice.path(), &url(&hub_a));
    bl(alice.path()).arg("remaster").arg("--detach").assert().success();
    assert!(
        !alice.path().join(".balls/state-repo").exists(),
        "detach must discard the hub clone (bl-692b)"
    );

    // Re-federate to hub B. Pre-fix the leftover clone was mistaken for
    // a warm cache and alice silently stayed on her hub-A-era orphan.
    remaster_commit(alice.path(), &url(&hub_b));

    // Alice is on hub B's `balls/tasks`: she sees bob's task.
    let listing = list_output(alice.path());
    assert!(
        listing.contains(&bob_id),
        "re-federated repo must adopt the new hub's tasks: {listing}"
    );

    // ...and `bl sync` round-trips onto hub B: a new task reaches bob.
    let alice_id = create_task(alice.path(), "alice's post-refederation task");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();
    assert!(
        list_output(bob.path()).contains(&alice_id),
        "bl sync after re-federation must push onto the new hub"
    );
}
