//! `bl remaster` edge cases under the unified tracker-state model:
//! the argument-error branches, seeding a fresh tracker, the legacy
//! `master.json` retirement, git-remote-name resolution, reconcile
//! id-clash renaming, and detach re-pointing `origin`.
//!
//! Phase 1B (bl-213e) flipped `cmd_init` to XDG; `bl remaster` itself
//! is still legacy-layout-only (Phase 1B-7 / bl-be70 makes it
//! XDG-aware). Every test below stands the clone up via
//! `legacy_clone()` — the hand-built pre-XDG scaffolding — so the
//! existing `bl remaster` code path stays under test until its
//! rewrite lands.

mod common;

use common::tracker::*;
use common::*;
use std::path::Path;

#[test]
fn remaster_detach_rejects_a_target() {
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");
    let assert = bl(&ws)
        .args(["remaster", "--detach", "some-url"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("takes no TARGET"), "{stderr}");
}

#[test]
fn remaster_on_a_non_balls_repo_errors() {
    let repo = new_repo(); // a git repo, but never `bl init`-ed
    let assert = bl(repo.path())
        .args(["remaster", &url_of(&new_tracker())])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("not a balls clone"), "{stderr}");
}

#[test]
fn remaster_without_a_target_errors() {
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");
    let assert = bl(&ws).arg("remaster").assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("needs a TARGET"), "{stderr}");
}

/// `bl remaster` requires a non-stealth git-backed repo. The legacy
/// stealth layout (`.balls/config.json` + `.balls/local/tasks_dir`
/// marker) is what gets past the wrapper's "is this a balls clone?"
/// pre-check; the deeper `store.stealth` guard then fires.
#[test]
fn remaster_rejects_a_stealth_repo() {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@t"]);
    git(dir.path(), &["config", "user.name", "t"]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    let tasks = dir.path().join("ext-tasks");
    balls::Store::init(
        dir.path(),
        true,
        Some(tasks.to_string_lossy().into_owned()),
    )
    .expect("legacy stealth init");
    let assert = bl(dir.path())
        .args(["remaster", &url_of(&new_tracker())])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("non-stealth"), "{stderr}");
}

#[test]
fn remaster_seeds_a_fresh_empty_tracker() {
    // `legacy_clone` plants a state-repo whose origin points at the
    // legacy clone's remote; `git remote set-url` later leaves stale
    // `refs/remotes/origin/balls/tasks` behind, so this fixture
    // hand-clears them before bl remaster runs (its
    // `has_remote_branch` check sees the live tracker, not the stale
    // ref). The Seeded path then triggers as expected.
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");
    create_task(&ws, "local task");
    let state_repo = discover_state_repo(&ws).expect("non-stealth state checkout");
    // Drop the stale remote-tracking ref so a re-pointed origin sees
    // the new tracker honestly.
    let _ = std::process::Command::new("git")
        .current_dir(&state_repo)
        .args(["update-ref", "-d", "refs/remotes/origin/balls/tasks"])
        .output();
    let empty = new_bare_remote();
    let out = bl(&ws)
        .args(["remaster", &url_of(&empty)])
        .assert()
        .success();
    let summary = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(summary.contains("seeded a fresh tracker"), "{summary}");
}

#[test]
fn remaster_unreachable_tracker_errors() {
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");
    let assert = bl(&ws)
        .args(["remaster", "/no/such/tracker.git"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("could not reach tracker"), "{stderr}");
}

#[test]
fn remaster_retires_a_legacy_master_json_pointer() {
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");
    // A pre-spec repo carries the retired pointer file.
    std::fs::write(
        ws.join(".balls/master.json"),
        r#"{"state_remote":"ghost"}"#,
    )
    .unwrap();
    bl(&ws)
        .args(["remaster", &url_of(&new_tracker())])
        .assert()
        .success();
    assert!(
        !ws.join(".balls/master.json").exists(),
        "remaster must retire the legacy pointer once the address folds in"
    );
}

#[test]
fn remaster_resolves_a_bare_git_remote_name() {
    let tracker = new_tracker();
    let home = tmp();
    let (_r, ws, _u) = legacy_clone(home.path(), "ws");
    git(&ws, &["remote", "add", "hub", &url_of(&tracker)]);

    bl(&ws).args(["remaster", "hub"]).assert().success();
    assert_eq!(
        state_url(&ws).as_deref(),
        Some(url_of(&tracker).as_str()),
        "a bare remote name resolves to its URL in the stored address"
    );
}

#[test]
fn remaster_reconcile_renames_an_id_clash() {
    let tracker = new_tracker();
    let home = tmp();
    // Alice seeds the tracker with one task.
    let (_r1, alice, _u1) = legacy_clone(home.path(), "alice");
    seed_config(&alice, &[("state_url", &url_of(&tracker))]);
    bl(&alice).arg("init").assert().success();
    let shared = create_task(&alice, "alice task");
    bl(&alice).arg("sync").assert().success();

    // Bob, standalone, creates a task and is surgically given the
    // SAME id — a genuine independent-creation clash.
    let (_r2, bob, _u2) = legacy_clone(home.path(), "bob");
    let bob_id = create_task(&bob, "bob task");
    clash_rename(&bob, &bob_id, &shared);

    let out = bl(&bob)
        .args(["remaster", &url_of(&tracker)])
        .assert()
        .success();
    let summary = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(summary.contains("renamed"), "reconcile summary: {summary}");

    let listed = bl(&bob).arg("list").assert().success();
    let listing = String::from_utf8(listed.get_output().stdout.clone()).unwrap();
    assert!(listing.contains(&shared), "alice's task adopted: {listing}");
    assert!(listing.contains("bob task"), "bob's task re-imported: {listing}");
}

/// Rewrite task `from`'s file in `repo`'s state checkout to id `to`,
/// committing the result — fabricates an id clash for reconcile.
fn clash_rename(repo: &Path, from: &str, to: &str) {
    let tasks = discover_tasks_dir(repo);
    let mut v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(tasks.join(format!("{from}.json"))).unwrap(),
    )
    .unwrap();
    v["id"] = serde_json::Value::String(to.to_string());
    std::fs::remove_file(tasks.join(format!("{from}.json"))).unwrap();
    std::fs::write(
        tasks.join(format!("{to}.json")),
        serde_json::to_string_pretty(&v).unwrap(),
    )
    .unwrap();
    commit_state_repo(repo, "fabricate id clash");
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

#[test]
fn detach_repoints_origin_at_the_code_remote() {
    let tracker = new_tracker();
    let home = tmp();
    let (_r, ws, code_url) = legacy_clone(home.path(), "ws");
    bl(&ws).args(["remaster", &url_of(&tracker)]).assert().success();

    bl(&ws).args(["remaster", "--detach"]).assert().success();
    let state_repo = discover_state_repo(&ws).expect("non-stealth state checkout");
    let origin = git(&state_repo, &["remote", "get-url", "origin"]);
    assert_eq!(
        origin.trim(),
        code_url,
        "detach re-points the state checkout at the code origin"
    );
}
