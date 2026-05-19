//! bl-c0c6 — `state_remote` decouples the `balls/tasks` orphan ref
//! from the code remote.
//!
//! Conformance (mirrors SPEC-lifecycle §17.1): with `state_remote`
//! unset or `"origin"`, every state-branch round-trip targets the
//! same remote as before this field — a single-repo setup is
//! byte-identical. The positive case: a client repo whose committed
//! config points `state_remote` at a separate task hub negotiates
//! `balls/tasks` against the hub while code stays on `origin`.

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// True if the bare repo at `bare` has the state branch ref.
fn bare_has_state_branch(bare: &Path) -> bool {
    git_ok(
        bare,
        &["rev-parse", "--verify", "--quiet", "refs/heads/balls/tasks"],
    )
}

/// Write a minimal valid `.balls/config.json` before `bl init` so the
/// repo is onboarded to a hub from its first lifecycle command — the
/// only way to set `state_remote` until `bl remaster` (bl-2057) lands.
fn seed_config(repo: &Path, state_remote: Option<&str>) {
    let balls = repo.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    let sr = match state_remote {
        Some(r) => format!(r#","state_remote":"{r}""#),
        None => String::new(),
    };
    fs::write(
        balls.join("config.json"),
        format!(
            r#"{{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"{sr}}}"#
        ),
    )
    .unwrap();
}

/// A client repo pointed at a task hub pushes `balls/tasks` to the hub
/// and never to the code remote; a second clone with the same
/// committed config adopts the hub's branch and sees the task.
#[test]
fn hub_topology_decouples_state_branch_from_code_remote() {
    let code = new_bare_remote();
    let hub = new_bare_remote();

    let alice = clone_from_remote(code.path(), "alice");
    git(
        alice.path(),
        &["remote", "add", "hub", &hub.path().to_string_lossy()],
    );
    seed_config(alice.path(), Some("hub"));
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["push", "origin", "main"]);

    let id = create_task(alice.path(), "hub task");
    bl(alice.path()).arg("sync").assert().success();
    // Re-publish main so bob clones the committed hub-linked config.
    git(alice.path(), &["push", "origin", "main"]);

    // The orphan ref lives on the hub, never on the code remote.
    assert!(
        bare_has_state_branch(hub.path()),
        "hub must carry balls/tasks"
    );
    assert!(
        !bare_has_state_branch(code.path()),
        "code remote must NOT carry balls/tasks — the two are decoupled"
    );

    let bob = clone_from_remote(code.path(), "bob");
    git(
        bob.path(),
        &["remote", "add", "hub", &hub.path().to_string_lossy()],
    );
    // bob's committed config (cloned from main) already names the hub;
    // bl init adopts the hub's branch instead of forking an orphan.
    bl(bob.path()).arg("init").assert().success();
    assert!(
        bob.path()
            .join(".balls/tasks")
            .join(format!("{id}.json"))
            .exists(),
        "bob must see alice's task via the shared hub"
    );
}

/// Conformance: an unset `state_remote` is byte-identical to before
/// the field — the state branch tracks the code remote, the key is
/// absent from the serialized config, and a vanilla clone round-trips.
#[test]
fn unset_state_remote_is_byte_identical_default() {
    let code = new_bare_remote();
    let hub = new_bare_remote();

    let alice = clone_from_remote(code.path(), "alice");
    bl(alice.path()).arg("init").assert().success();
    let id = create_task(alice.path(), "default task");
    push(alice.path());

    let cfg = fs::read_to_string(alice.path().join(".balls/config.json")).unwrap();
    assert!(
        !cfg.contains("state_remote"),
        "unset state_remote must not serialize: {cfg}"
    );
    assert!(
        bare_has_state_branch(code.path()),
        "default setup keeps balls/tasks on the code remote"
    );
    assert!(
        !bare_has_state_branch(hub.path()),
        "an unrelated remote stays empty"
    );

    let bob = clone_from_remote(code.path(), "bob");
    bl(bob.path()).arg("init").assert().success();
    assert!(
        bob.path()
            .join(".balls/tasks")
            .join(format!("{id}.json"))
            .exists(),
        "vanilla clone round-trips the task via origin"
    );
}

/// `state_remote: "origin"` is explicitly the same as unset: the
/// state branch still tracks the code remote.
#[test]
fn explicit_origin_state_remote_equals_unset() {
    let code = new_bare_remote();
    let alice = clone_from_remote(code.path(), "alice");
    seed_config(alice.path(), Some("origin"));
    bl(alice.path()).arg("init").assert().success();
    create_task(alice.path(), "explicit origin");
    bl(alice.path()).arg("sync").assert().success();
    assert!(
        bare_has_state_branch(code.path()),
        "state_remote=origin behaves exactly like unset"
    );
}
