//! bl-8e8f — `bl init` safety against a configured `state_remote`.
//!
//! Guarantees:
//!  - a fresh clone whose committed config names a *reachable* hub
//!    adopts the hub's `balls/tasks` (silent);
//!  - an unaware clone (config names a hub it has no git remote for)
//!    gets a usable *isolated* local store plus a clear advisory — it
//!    is never a destructive surprise;
//!  - init never clobbers a non-empty shared branch, even when a
//!    divergent local orphan exists and a push is attempted;
//!  - an unset `state_remote` is exactly today's behavior (no
//!    advisory).

mod common;

use common::*;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

const ADVISORY: &str = "isolated local task store";

fn bare_state_sha(bare: &Path) -> String {
    git(bare, &["rev-parse", "refs/heads/balls/tasks"])
        .trim()
        .to_string()
}

fn seed_config(repo: &Path, state_remote: &str) {
    let balls = repo.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    fs::write(
        balls.join("config.json"),
        format!(
            r#"{{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees","state_remote":"{state_remote}"}}"#
        ),
    )
    .unwrap();
}

/// Onboard a hub: alice (origin=code, remote hub) creates a task and
/// publishes the committed hub-linked config to the code remote.
/// Returns (code, hub, task id).
fn onboarded_project() -> (Repo, Repo, String) {
    let code = new_bare_remote();
    let hub = new_bare_remote();
    let alice = clone_from_remote(code.path(), "alice");
    git(
        alice.path(),
        &["remote", "add", "hub", &hub.path().to_string_lossy()],
    );
    seed_config(alice.path(), "hub");
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["push", "origin", "main"]);
    let id = create_task(alice.path(), "shared task");
    bl(alice.path()).arg("sync").assert().success();
    git(alice.path(), &["push", "origin", "main"]);
    (code, hub, id)
}

#[test]
fn adopt_configured_reachable_state_remote_is_silent() {
    let (code, hub, id) = onboarded_project();
    let bob = clone_from_remote(code.path(), "bob");
    git(
        bob.path(),
        &["remote", "add", "hub", &hub.path().to_string_lossy()],
    );
    bl(bob.path())
        .arg("init")
        .assert()
        .success()
        .stderr(predicate::str::contains(ADVISORY).not());
    assert!(
        bob.path()
            .join(".balls/tasks")
            .join(format!("{id}.json"))
            .exists(),
        "a clone with the hub remote adopts the shared branch"
    );
}

#[test]
fn unaware_clone_creates_isolated_store_and_advises() {
    let (code, hub, shared) = onboarded_project();
    let hub_sha = bare_state_sha(hub.path());

    // bob clones the code repo (committed config names `hub`) but
    // never configures a `hub` git remote.
    let bob = clone_from_remote(code.path(), "bob");
    bl(bob.path())
        .arg("init")
        .assert()
        .success()
        .stderr(predicate::str::contains(ADVISORY))
        .stderr(predicate::str::contains("bl remaster hub"));

    // The isolated store is fully usable...
    let local = create_task(bob.path(), "bob local");
    bl(bob.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(&local));
    // ...but it is isolated: the shared task is not here, and the hub
    // ref was not touched (bob has no way to reach it).
    assert!(
        !bob.path()
            .join(".balls/tasks")
            .join(format!("{shared}.json"))
            .exists(),
        "unaware init must not magically pull shared tasks"
    );
    assert_eq!(
        hub_sha,
        bare_state_sha(hub.path()),
        "unaware init must not mutate the hub"
    );
}

#[test]
fn init_never_clobbers_non_empty_shared_branch() {
    let (code, hub, _shared) = onboarded_project();
    let hub_sha_before = bare_state_sha(hub.path());

    // carol first inits with no hub remote → isolated local orphan.
    let carol = clone_from_remote(code.path(), "carol");
    bl(carol.path()).arg("init").assert().success();
    create_task(carol.path(), "carol divergent");

    // Now carol adds the hub remote and re-inits. The local orphan
    // already exists and is unrelated to the hub's history; init's
    // best-effort push is a non-fast-forward git rejects. The hub
    // must be byte-for-byte unchanged — never force-pushed.
    git(
        carol.path(),
        &["remote", "add", "hub", &hub.path().to_string_lossy()],
    );
    bl(carol.path()).arg("init").assert().success();
    assert_eq!(
        hub_sha_before,
        bare_state_sha(hub.path()),
        "init must never clobber a non-empty shared branch"
    );
}

#[test]
fn unset_state_remote_init_has_no_advisory() {
    // A plain local repo with no state_remote: today's behavior,
    // never the not-joined advisory even with no reachable remote.
    let repo = new_repo();
    bl(repo.path())
        .arg("init")
        .assert()
        .success()
        .stderr(predicate::str::contains(ADVISORY).not());
}
