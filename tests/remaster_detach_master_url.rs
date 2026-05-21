//! bl-f440 — warm `bl remaster --detach` under `master_url` must
//! leave the repo on the state worktree a fresh `Store::discover`
//! resolves.
//!
//! Detach clears `master_url`, after which `discover` resolves the
//! legacy `.balls/worktree`. But the warm detach re-roots the orphan
//! inside the balls-owned `.balls/state-repo` clone — so without the
//! transplant the detached task set is either silently invisible (a
//! stale `.balls/worktree` from a pre-flip standalone era) or the
//! repo hard-fails discovery (a seeded-federated repo never had one).
//!
//! Both shapes are covered: a `bl list` / `bl create` after detach
//! must see the detached tasks, not error and not read a stale tree.

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// `bl list` stdout for `repo` (asserts the command succeeded).
fn list_output(repo: &Path) -> String {
    let out = bl(repo).arg("list").assert().success();
    String::from_utf8(out.get_output().stdout.clone()).unwrap()
}

/// Seed a non-stealth config carrying `master_url` so `bl init` takes
/// the federated leg directly — no `.balls/worktree` is ever created.
fn seed_master_url_config(repo: &Path, hub_url: &str) {
    let balls = repo.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    fs::write(
        balls.join("config.json"),
        format!(
            r#"{{"version":1,"id_length":4,"stale_threshold_seconds":60,
                 "worktree_dir":".balls-worktrees","master_url":"{hub_url}"}}"#
        ),
    )
    .unwrap();
}

/// A standalone repo flipped to a federated hub leaves `.balls/worktree`
/// behind as a stale pre-flip leftover. Warm detach must transplant the
/// federated state onto it, not let `discover` keep reading the stale
/// tree.
#[test]
fn warm_detach_after_flip_makes_federated_tasks_visible() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path()); // standalone — creates the legacy .balls/worktree

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();
    // Created while federated — lands in the balls-owned state-repo.
    let fed_id = create_task(alice.path(), "federated task");

    bl(alice.path())
        .arg("remaster")
        .arg("--detach")
        .assert()
        .success();

    assert!(
        alice.path().join(".balls/worktree/.balls/tasks").exists(),
        "detach must leave the legacy state worktree as the live store"
    );
    assert!(
        list_output(alice.path()).contains(&fed_id),
        "detached repo must see the federated task, not a stale .balls/worktree"
    );
    // The store is writable post-detach: a fresh create round-trips.
    let post_id = create_task(alice.path(), "post-detach task");
    let listing = list_output(alice.path());
    assert!(
        listing.contains(&fed_id) && listing.contains(&post_id),
        "post-detach store must carry both the federated and new tasks: {listing}"
    );
}

/// A repo initialized directly against a `master_url` config never had
/// a `.balls/worktree`. Warm detach must create one carrying the
/// detached state — otherwise `discover` hard-fails on the missing
/// worktree and the repo is bricked.
#[test]
fn warm_detach_of_seeded_federated_repo_creates_the_worktree() {
    let hub = new_bare_remote();
    let alice = new_repo();
    seed_master_url_config(alice.path(), hub.path().to_string_lossy().as_ref());
    bl(alice.path()).arg("init").assert().success();
    assert!(
        !alice.path().join(".balls/worktree").exists(),
        "a seeded-federated init must not create a .balls/worktree"
    );

    let fed_id = create_task(alice.path(), "federated task");

    bl(alice.path())
        .arg("remaster")
        .arg("--detach")
        .assert()
        .success();

    assert!(
        alice.path().join(".balls/worktree/.balls/tasks").exists(),
        "detach must create the legacy state worktree"
    );
    assert!(
        list_output(alice.path()).contains(&fed_id),
        "detached repo must see the federated task without erroring"
    );
    // A fresh create proves the new worktree is a usable store.
    let post_id = create_task(alice.path(), "post-detach task");
    assert!(
        list_output(alice.path()).contains(&post_id),
        "post-detach store must accept new tasks"
    );
}
