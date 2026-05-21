//! `bl remaster` edge cases under the unified tracker-state model:
//! the argument-error branches, seeding a fresh tracker, the legacy
//! `master.json` retirement, git-remote-name resolution, reconcile
//! id-clash renaming, and detach re-pointing `origin`.

mod common;

use common::tracker::*;
use common::*;
use std::path::Path;

#[test]
fn remaster_detach_rejects_a_target() {
    let repo = new_repo();
    init_in(repo.path());
    let assert = bl(repo.path())
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
    assert!(stderr.contains("not a balls workspace"), "{stderr}");
}

#[test]
fn remaster_without_a_target_errors() {
    let repo = new_repo();
    init_in(repo.path());
    let assert = bl(repo.path()).arg("remaster").assert().failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("needs a TARGET"), "{stderr}");
}

#[test]
fn remaster_rejects_a_stealth_repo() {
    let repo = new_repo();
    let tasks = repo.path().join("ext-tasks");
    bl(repo.path())
        .args(["init", "--tasks-dir"])
        .arg(&tasks)
        .assert()
        .success();
    let assert = bl(repo.path())
        .args(["remaster", &url_of(&new_tracker())])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("non-stealth"), "{stderr}");
}

#[test]
fn remaster_seeds_a_fresh_empty_tracker() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "local task");
    // A bare repo with no balls/tasks branch — a fresh tracker.
    let empty = new_bare_remote();
    let out = bl(repo.path())
        .args(["remaster", &url_of(&empty)])
        .assert()
        .success();
    let summary = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(summary.contains("seeded a fresh tracker"), "{summary}");
}

#[test]
fn remaster_unreachable_tracker_errors() {
    let repo = new_repo();
    init_in(repo.path());
    let assert = bl(repo.path())
        .args(["remaster", "/no/such/tracker.git"])
        .assert()
        .failure();
    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert!(stderr.contains("could not reach tracker"), "{stderr}");
}

#[test]
fn remaster_retires_a_legacy_master_json_pointer() {
    let repo = new_repo();
    init_in(repo.path());
    // A pre-spec repo carries the retired pointer file.
    std::fs::write(
        repo.path().join(".balls/master.json"),
        r#"{"state_remote":"ghost"}"#,
    )
    .unwrap();
    bl(repo.path())
        .args(["remaster", &url_of(&new_tracker())])
        .assert()
        .success();
    assert!(
        !repo.path().join(".balls/master.json").exists(),
        "remaster must retire the legacy pointer once the address folds in"
    );
}

#[test]
fn remaster_resolves_a_bare_git_remote_name() {
    let tracker = new_tracker();
    let repo = new_repo();
    init_in(repo.path());
    git(repo.path(), &["remote", "add", "hub", &url_of(&tracker)]);

    bl(repo.path()).args(["remaster", "hub"]).assert().success();
    assert_eq!(
        state_url(repo.path()).as_deref(),
        Some(url_of(&tracker).as_str()),
        "a bare remote name resolves to its URL in the stored address"
    );
}

#[test]
fn remaster_reconcile_renames_an_id_clash() {
    let tracker = new_tracker();
    // Alice seeds the tracker with one task.
    let alice = new_repo();
    seed_config(alice.path(), &[("state_url", &url_of(&tracker))]);
    bl(alice.path()).arg("init").assert().success();
    let shared = create_task(alice.path(), "alice task");
    bl(alice.path()).arg("sync").assert().success();

    // Bob, standalone, creates a task and is surgically given the
    // SAME id — a genuine independent-creation clash.
    let bob = new_repo();
    init_in(bob.path());
    let bob_id = create_task(bob.path(), "bob task");
    clash_rename(bob.path(), &bob_id, &shared);

    let out = bl(bob.path())
        .args(["remaster", &url_of(&tracker)])
        .assert()
        .success();
    let summary = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(summary.contains("renamed"), "reconcile summary: {summary}");

    let listed = bl(bob.path()).arg("list").assert().success();
    let listing = String::from_utf8(listed.get_output().stdout.clone()).unwrap();
    assert!(listing.contains(&shared), "alice's task adopted: {listing}");
    assert!(listing.contains("bob task"), "bob's task re-imported: {listing}");
}

/// Rewrite task `from`'s file in `repo`'s state checkout to id `to`,
/// committing the result — fabricates an id clash for reconcile.
fn clash_rename(repo: &Path, from: &str, to: &str) {
    let tasks = repo.join(".balls/state-repo/.balls/tasks");
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

#[test]
fn detach_repoints_origin_at_the_code_remote() {
    let code = new_bare_remote();
    let tracker = new_tracker();
    let ws = clone_from_remote(code.path(), "ws");
    init_in(ws.path());
    bl(ws.path()).args(["remaster", &url_of(&tracker)]).assert().success();

    bl(ws.path()).args(["remaster", "--detach"]).assert().success();
    let state_repo = ws.path().join(".balls/state-repo");
    let origin = git(&state_repo, &["remote", "get-url", "origin"]);
    assert_eq!(
        origin.trim(),
        url_of(&code),
        "detach re-points the state checkout at the code origin"
    );
}
