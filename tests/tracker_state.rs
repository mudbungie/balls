//! `bl remaster` argument-error and resolution edges under XDG.
//!
//! Phase 1B-7 (bl-be70) flipped `bl remaster` to write
//! `.balls/tracker.json` on the code repo's own `balls/tasks` branch
//! (SPEC §6.1). The legacy "reconcile local-only tasks onto the new
//! tracker" path was retired with the legacy `state_url`/`state_repo`
//! mechanism; tracker materialization is now `Store::discover`'s job
//! on next invocation. The remaining CLI surface is the argument
//! validation, layout/stealth pre-checks, and remote-name resolution.
//!
//! End-to-end XDG file-write conformance lives in
//! `tests/conformance_remaster.rs`; this file covers the negative
//! paths.

mod common;

use balls::encoding::{canonicalize_origin, percent_encode_component};
use balls::tracker_json::TrackerJson;
use balls::xdg_paths::own_tracker_checkout;
use common::tracker::*;
use common::*;
use std::fs;

#[test]
fn remaster_detach_rejects_a_target() {
    let xdg = new_xdg_repo();
    let assert = bl(xdg.clone.path())
        .args(["remaster", "--detach", "some-url"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("takes no TARGET"), "{stderr}");
}

#[test]
fn remaster_on_a_non_balls_repo_errors() {
    // A fresh repo that was never `bl init`ed — `Store::discover`
    // fails before remaster's XDG pre-check fires, with the standard
    // not-initialized diagnostic.
    let repo = new_repo();
    let assert = bl(repo.path())
        .args(["remaster", &url_of(&new_tracker())])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("not initialized") || stderr.contains("no .balls"),
        "expected a discovery failure on a non-balls repo, got: {stderr}"
    );
}

#[test]
fn remaster_without_a_target_errors() {
    let xdg = new_xdg_repo();
    let assert = bl(xdg.clone.path()).arg("remaster").assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("needs a TARGET"), "{stderr}");
}

/// A pre-XDG clone (still on the legacy `.balls/` layout) gets a
/// clean "run `bl migrate`" diagnostic — `bl remaster` no longer
/// writes the legacy `state_url` field.
#[test]
fn remaster_rejects_legacy_layout() {
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");
    let assert = bl(&ws)
        .args(["remaster", &url_of(&new_tracker())])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(
        stderr.contains("XDG layout") && stderr.contains("bl migrate"),
        "expected an XDG-required diagnostic, got: {stderr}"
    );
}

/// A stealth clone has no tracker checkout to write to, so remaster
/// refuses before reaching the file-write step.
#[test]
fn remaster_rejects_a_stealth_repo() {
    let dir = tmp();
    let tasks_path = std::fs::canonicalize(dir.path()).unwrap();
    bl(&tasks_path)
        .args(["init", "--tasks-dir", tasks_path.to_str().unwrap()])
        .assert()
        .success();
    let assert = bl(&tasks_path)
        .args(["remaster", &url_of(&new_tracker())])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("stealth"), "{stderr}");
}

/// `bl remaster hub` resolves the bare remote name to its URL on the
/// code repo and stores the URL in `tracker.json` (the file always
/// carries a fetchable URL, never a remote shortname — SPEC §6.1).
#[test]
fn remaster_resolves_a_bare_git_remote_name() {
    let xdg = new_xdg_repo();
    let tracker = new_bare_remote();
    let tracker_url = tracker.path().to_string_lossy().into_owned();
    git(xdg.clone.path(), &["remote", "add", "hub", &tracker_url]);

    bl(xdg.clone.path())
        .args(["remaster", "hub", "--commit"])
        .assert()
        .success();

    let enc = percent_encode_component(&canonicalize_origin(
        &xdg.remote.path().to_string_lossy(),
    ));
    let own = own_tracker_checkout(&test_xdg_bases(), &enc);
    let tj = own.join(".balls/tracker.json");
    let parsed = TrackerJson::from_json(&fs::read_to_string(&tj).unwrap()).unwrap();
    assert_eq!(
        parsed.state_url, tracker_url,
        "a bare remote name resolves to its URL in the stored address"
    );
}

/// Backstop for the legacy worktree-link path: `bl claim` on a
/// legacy clone plants the `.balls/state-repo` and `.balls/tasks`
/// symlinks inside the worktree (Phase 1A topology). XDG claim's
/// Layout::Xdg guard skips this; the legacy path is otherwise
/// uncovered post-flip.
#[test]
fn legacy_clone_claim_links_state_into_worktree() {
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");
    let id = create_task(&ws, "claim me");
    bl_as(&ws, "alice").args(["claim", &id]).assert().success();
    let wt = worktree_path(&ws, &id);
    assert!(wt.exists(), "worktree exists");
    assert!(
        wt.join(".balls/state-repo").is_symlink(),
        "legacy claim links state-repo into the worktree"
    );
    assert!(
        wt.join(".balls/tasks").is_symlink(),
        "legacy claim links tasks into the worktree"
    );
}
