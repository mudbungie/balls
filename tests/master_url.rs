//! bl-ffb4 — `master_url` carries a committed hub URL so a fresh
//! `git clone` plus `bl prime` joins the shared task store with no
//! manual `git remote add` and no project-side remote pollution.
//!
//! Conformance:
//! - A repo with committed `master_url` materializes `.balls/state-repo/`
//!   on first lifecycle command and routes state-branch ops through it.
//! - The project's own `.git/config` never grows a hub remote — the URL
//!   lives only inside `.balls/state-repo/.git/config`.
//! - `bl remaster <URL> --commit` auto-routes a URL target to
//!   `master_url`, distinct from the legacy `state_remote` name path.

mod common;

use common::*;
use std::fs;
use std::path::Path;

fn read_master_url(repo: &Path) -> Option<String> {
    let s = fs::read_to_string(repo.join(".balls/config.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    v.get("master_url")?.as_str().map(String::from)
}

fn state_repo_origin_url(repo: &Path) -> String {
    git(&repo.join(".balls/state-repo"), &["remote", "get-url", "origin"])
        .trim()
        .to_string()
}

fn project_remotes(repo: &Path) -> Vec<String> {
    git(repo, &["remote"]).lines().map(String::from).collect()
}

/// `bl remaster <hub-url> --commit` writes the URL to committed config,
/// materializes the balls-owned state-repo, and leaves the project's
/// own `.git/config` clean.
#[test]
fn remaster_with_url_writes_master_url_and_isolates_project_git() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    create_task(alice.path(), "seed");

    let hub_url = hub.path().to_string_lossy().to_string();
    bl(alice.path())
        .arg("remaster")
        .arg(&hub_url)
        .arg("--commit")
        .assert()
        .success();

    assert_eq!(
        read_master_url(alice.path()).as_deref(),
        Some(hub_url.as_str()),
        "committed config must carry master_url"
    );
    assert!(
        alice.path().join(".balls/state-repo/.git").exists(),
        "balls must materialize its own state-repo on remaster --commit"
    );
    assert_eq!(
        state_repo_origin_url(alice.path()),
        hub_url,
        "state-repo's own origin URL is the hub"
    );
    assert!(
        !project_remotes(alice.path()).contains(&"hub".to_string()),
        "project's git remote -v must NOT grow a hub remote: got {:?}",
        project_remotes(alice.path())
    );
}

/// A fresh `git clone` of a project whose committed config sets
/// `master_url` auto-provisions `.balls/state-repo/` on the next
/// lifecycle command — no manual `git remote add` and no `bl remaster`
/// required.
#[test]
fn fresh_clone_with_master_url_auto_provisions_on_prime() {
    let code = new_bare_remote();
    let hub = new_bare_remote();
    let hub_url = hub.path().to_string_lossy().to_string();

    // Onboarding clone: init, point master_url at the hub, publish.
    let onboard = clone_from_remote(code.path(), "onboard");
    init_in(onboard.path());
    bl(onboard.path())
        .arg("remaster")
        .arg(&hub_url)
        .arg("--commit")
        .assert()
        .success();
    let id = create_task(onboard.path(), "shared task");
    bl(onboard.path()).arg("sync").assert().success();
    // bl-ebae: `remaster --commit` commits the master_url flip itself,
    // so the onboarding clone only has to publish `main`.
    git(onboard.path(), &["push", "origin", "main"]);

    // Teammate: a fresh `git clone` with no hub remote configured.
    let teammate = clone_from_remote(code.path(), "teammate");
    assert!(
        !teammate.path().join(".balls/state-repo").exists(),
        "fresh clone has no state-repo yet"
    );
    assert!(
        !project_remotes(teammate.path()).contains(&"hub".to_string()),
        "fresh clone has no project-side hub remote"
    );

    // `bl prime` is the bootstrap moment. It must materialize the
    // state-repo from committed master_url and the teammate sees the
    // shared task.
    bl(teammate.path()).arg("prime").assert().success();
    assert!(
        teammate.path().join(".balls/state-repo/.git").exists(),
        "bl prime must materialize the state-repo from committed master_url"
    );

    let list = bl(teammate.path()).arg("list").assert().success();
    let stdout = String::from_utf8(list.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains(&id),
        "fresh clone must see the shared task after prime: {stdout}"
    );
}

/// bl-16e9 regression: `bl sync` must push `balls/tasks` to the hub
/// when `master_url` is set even if the project root has no `origin`.
/// The state-leg presence gate used to ask the project root whether
/// the state remote exists; in `master_url` mode the remote lives
/// inside `.balls/state-repo/.git/config`, not the project's, so the
/// push was silently skipped for the exact topology `master_url` was
/// built to enable (e.g. a bridge clone with no code remote at all).
#[test]
fn sync_pushes_state_branch_in_master_url_mode_with_no_project_origin() {
    let hub = new_bare_remote();
    let hub_url = hub.path().to_string_lossy().to_string();
    let alice = new_repo();
    let balls = alice.path().join(".balls");
    fs::create_dir_all(&balls).unwrap();
    fs::write(
        balls.join("config.json"),
        format!(
            r#"{{"version":1,"id_length":4,"stale_threshold_seconds":60,
                 "worktree_dir":".balls-worktrees","master_url":"{hub_url}"}}"#
        ),
    )
    .unwrap();
    bl(alice.path()).arg("init").assert().success();
    assert!(
        !project_remotes(alice.path()).contains(&"origin".to_string()),
        "project root must have no `origin`: {:?}",
        project_remotes(alice.path())
    );

    let id = create_task(alice.path(), "shared task");
    bl(alice.path()).arg("sync").assert().success();

    let listing = git(hub.path(), &["ls-tree", "-r", "--name-only", "balls/tasks"]);
    assert!(
        listing.contains(&format!(".balls/tasks/{id}.json")),
        "hub must carry the synced task on `balls/tasks`: {listing}"
    );
}

/// `bl remaster <url>` *without* `--commit` materializes the
/// state-repo locally but leaves the committed config untouched —
/// the per-clone path.
#[test]
fn remaster_url_per_clone_materializes_without_committing() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .assert()
        .success();

    assert!(
        alice.path().join(".balls/state-repo/.git").exists(),
        "per-clone remaster must still materialize the state-repo"
    );
    assert!(
        read_master_url(alice.path()).is_none(),
        "per-clone (no --commit) must NOT write master_url to committed config"
    );
}

/// `bl init` against a project whose seeded `.balls/config.json`
/// already sets `master_url` must materialize the state-repo during
/// init, not wait for the next command. Exercises the init-time
/// branch in `Store::init` (distinct from `bl prime`'s discover-time
/// branch).
#[test]
fn bl_init_with_seeded_master_url_materializes_state_repo() {
    let hub = new_bare_remote();
    let hub_url = hub.path().to_string_lossy().to_string();
    let alice = new_repo();
    let balls = alice.path().join(".balls");
    fs::create_dir_all(&balls).unwrap();
    fs::write(
        balls.join("config.json"),
        format!(
            r#"{{"version":1,"id_length":4,"stale_threshold_seconds":60,
                 "worktree_dir":".balls-worktrees","master_url":"{hub_url}"}}"#
        ),
    )
    .unwrap();

    bl(alice.path()).arg("init").assert().success();
    assert!(
        alice.path().join(".balls/state-repo/.git").exists(),
        "bl init must materialize state-repo when committed config sets master_url"
    );
}

/// `bl remaster --detach` clears `master_url` so a forked project drops
/// the hub link cleanly.
#[test]
fn remaster_detach_clears_master_url() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();
    assert!(read_master_url(alice.path()).is_some());

    bl(alice.path()).arg("remaster").arg("--detach").assert().success();
    assert!(
        read_master_url(alice.path()).is_none(),
        "detach must clear master_url from committed config"
    );
}

// The bl-dcd3 hard-fail tests for an unreachable hub live in
// `tests/master_url_hard_fail.rs` — split out to keep both files
// under the 300-line cap.
