//! Tests for the optional remote-sync-on-claim policy (bl-2148).

mod common;

use common::*;
use std::fs;

fn three_way() -> (Repo, Repo, Repo) {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    bl(alice.path()).arg("init").assert().success();
    push(alice.path());

    let bob = clone_from_remote(remote.path(), "bob");
    bl(bob.path()).arg("init").assert().success();
    (remote, alice, bob)
}

fn flip_repo_policy_on(repo: &std::path::Path) {
    let cfg_path = repo.join(".balls/config.json");
    let cfg = fs::read_to_string(&cfg_path).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(&cfg).unwrap();
    j["require_remote_on_claim"] = serde_json::Value::Bool(true);
    fs::write(&cfg_path, serde_json::to_string_pretty(&j).unwrap()).unwrap();
    git(repo, &["add", ".balls/config.json"]);
    git(repo, &["commit", "-m", "policy: require remote on claim"]);
}

fn break_remote(repo: &std::path::Path) {
    git(
        repo,
        &["remote", "set-url", "origin", "/tmp/balls-no-such-remote.git"],
    );
}

#[test]
fn sync_flag_pushes_claim_to_remote() {
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "shared");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id, "--sync"])
        .assert()
        .success();

    // Bob's plain `bl sync` picks up alice's claim, with no extra
    // `bl sync` call from alice in between — that's the whole point.
    bl(bob.path()).arg("sync").assert().success();
    let j = read_task_json(bob.path(), &id);
    assert_eq!(j["claimed_by"], "alice");
    assert_eq!(j["status"], "in_progress");
}

#[test]
fn sync_flag_loses_to_earlier_claim() {
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "race");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    // Alice claims first with --sync; commit lands on origin.
    bl_as(alice.path(), "alice")
        .args(["claim", &id, "--sync"])
        .assert()
        .success();

    // Bob's local state branch is behind. He attempts claim --sync;
    // push is rejected, the merge resolves alice as winner, bob's
    // claim fails loudly.
    let out = bl_as(bob.path(), "bob")
        .args(["claim", &id, "--sync"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected bob's claim to fail");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("alice") || stderr.contains("already claimed"),
        "stderr: {stderr}"
    );

    // Bob's local task file now shows alice as claimant.
    let j = read_task_json(bob.path(), &id);
    assert_eq!(j["claimed_by"], "alice");

    // Bob has no worktree and no claim file.
    assert!(!bob.path().join(".balls-worktrees").join(&id).exists());
    assert!(!bob.path().join(".balls/local/claims").join(&id).exists());
}

#[test]
fn sync_flag_fails_loud_on_unreachable_remote() {
    let (_r, alice, _bob) = three_way();
    let id = create_task(alice.path(), "offline");
    break_remote(alice.path());

    let out = bl_as(alice.path(), "alice")
        .args(["claim", &id, "--sync"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("unreachable") || stderr.contains("fetch failed"),
        "stderr: {stderr}"
    );

    // Task rolled back to open — local claim commit reverted.
    let j = read_task_json(alice.path(), &id);
    assert_eq!(j["status"], "open");
    assert!(j["claimed_by"].is_null());
    assert!(!alice.path().join(".balls-worktrees").join(&id).exists());
}

#[test]
fn no_sync_flag_overrides_repo_default() {
    let (_r, alice, _bob) = three_way();
    flip_repo_policy_on(alice.path());
    let id = create_task(alice.path(), "offline-by-choice");

    break_remote(alice.path());
    bl_as(alice.path(), "alice")
        .args(["claim", &id, "--no-sync"])
        .assert()
        .success();
}

#[test]
fn default_off_claim_works_offline() {
    let (_r, alice, _bob) = three_way();
    let id = create_task(alice.path(), "offline");
    break_remote(alice.path());
    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
}

#[test]
fn repo_config_default_drives_claim_to_remote() {
    let (_r, alice, _bob) = three_way();
    flip_repo_policy_on(alice.path());
    let id = create_task(alice.path(), "policy task");

    // Without a CLI flag, the repo-default policy kicks in. Break
    // the remote and the claim should fail loudly — no silent
    // fallback to local-only.
    break_remote(alice.path());
    let out = bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected policy-driven claim to fail offline");
    let j = read_task_json(alice.path(), &id);
    assert_eq!(j["status"], "open");
}

#[test]
fn local_config_overrides_repo_default_off() {
    let (_r, alice, _bob) = three_way();
    flip_repo_policy_on(alice.path());

    // Local override flips it off for this clone only.
    let local_cfg = alice.path().join(".balls/local/config.json");
    fs::write(
        &local_cfg,
        r#"{"require_remote_on_claim": false}"#,
    )
    .unwrap();

    let id = create_task(alice.path(), "local-off");
    break_remote(alice.path());
    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
}

#[test]
fn cli_sync_and_no_sync_conflict() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "x");
    let out = bl(repo.path())
        .args(["claim", &id, "--sync", "--no-sync"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "clap should reject conflicting flags");
}

#[test]
fn no_worktree_claim_also_syncs() {
    let (_r, alice, _bob) = three_way();
    let id = create_task(alice.path(), "no-wt");
    bl(alice.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id, "--no-worktree", "--sync"])
        .assert()
        .success();

    // The claim commit must be reachable from origin/balls/tasks.
    let s = git(alice.path(), &["log", "--format=%s", "origin/balls/tasks"]);
    assert!(
        s.lines().any(|l| l.contains(&format!("balls: claim {id}"))),
        "expected claim commit on origin/balls/tasks, got:\n{s}"
    );
}

#[test]
fn prime_announces_repo_default_once() {
    let (_r, alice, _bob) = three_way();
    flip_repo_policy_on(alice.path());

    // Force config visibility: alice's local clone has the policy in
    // its checked-out config, and no marker file yet.
    let marker = alice.path().join(".balls/local/seen-claim-sync-policy");
    let _ = fs::remove_file(&marker);

    let out1 = bl(alice.path()).arg("prime").output().unwrap();
    let s1 = String::from_utf8_lossy(&out1.stderr).to_string();
    assert!(
        s1.contains("synced claims") || s1.contains("require_remote_on_claim"),
        "first prime should hint, got: {s1}"
    );
    assert!(marker.exists(), "marker should be written after first hint");

    let out2 = bl(alice.path()).arg("prime").output().unwrap();
    let s2 = String::from_utf8_lossy(&out2.stderr).to_string();
    assert!(
        !s2.contains("synced claims") && !s2.contains("require_remote_on_claim"),
        "second prime should be silent, got: {s2}"
    );
}
