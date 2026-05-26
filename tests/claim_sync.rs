//! Tests for the optional remote-sync-on-claim policy (bl-2148).

mod common;

use common::*;
use common::multidev::*;
use std::fs;

fn flip_repo_policy_on(repo: &std::path::Path) {
    let cfg_path = repo.join(".balls/config.json");
    let cfg = fs::read_to_string(&cfg_path).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(&cfg).unwrap();
    j["require_remote_on_claim"] = serde_json::Value::Bool(true);
    fs::write(&cfg_path, serde_json::to_string_pretty(&j).unwrap()).unwrap();
    git(repo, &["add", ".balls/config.json"]);
    git(repo, &["commit", "-m", "policy: require remote on claim"]);
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
    assert!(!worktree_path(bob.path(), &id).exists());
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
    assert!(!worktree_path(alice.path(), &id).exists());
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
fn legacy_local_config_no_longer_overrides_and_doctor_flags_it() {
    // bl-5a03 retired the `.balls/local/config.json` reader. On a
    // legacy clone the file just sits there — claim ignores it (so
    // the repo-default policy wins, not the override) and `bl doctor`
    // surfaces the file so the user knows to translate it into
    // `clone.json` after migrating.
    let (_r, alice, _bob) = three_way();
    flip_repo_policy_on(alice.path());

    // The legacy local override file claims to flip require_remote
    // off for this clone — but bl no longer reads it.
    fs::write(
        alice.path().join(".balls/local/config.json"),
        r#"{"require_remote_on_claim": false}"#,
    )
    .unwrap();

    let id = create_task(alice.path(), "local-off");
    break_remote(alice.path());

    // With the reader gone, the repo default (require_remote_on_claim
    // = true) still drives the policy. The broken remote makes the
    // claim fail — exactly as if no override file existed.
    let out = bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "expected claim to fail under repo-default policy; legacy local/config.json must be ignored"
    );

    // Doctor surfaces the legacy file as a finding so the user knows
    // the override is silently dead.
    let report = doctor(alice.path());
    assert!(
        report.contains(".balls/local/config.json")
            && report.contains("pre-XDG"),
        "expected `bl doctor` to flag legacy local/config.json; got:\n{report}"
    );
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
    let s = git_state(alice.path(), &["log", "--format=%s", "origin/balls/tasks"]);
    assert!(
        s.lines().any(|l| l.contains(&format!("balls: claim {id}"))),
        "expected claim commit on origin/balls/tasks, got:\n{s}"
    );
}

#[test]
fn claim_announces_repo_default_sync() {
    // Reactive sync-notice (bl-1432): the line lands on stderr when
    // `bl claim` is *about to* round-trip because of the repo default,
    // not preemptively at prime. Three cases, one fixture each:
    //   1. repo default on, no override        → notice present
    //   2. --no-sync overrides                 → notice absent
    //   3. clone-level override turns it off   → notice absent
    let notice_hit = |s: &str| {
        s.contains("syncing claim through origin/balls/tasks")
            && s.contains("repo default")
    };

    // (1) Repo default kicks in.
    let (_r1, alice1, bob1) = three_way();
    flip_repo_policy_on(alice1.path());
    bl(bob1.path()).arg("sync").assert().success();
    let id1 = create_task(alice1.path(), "default-on");
    let out1 = bl_as(alice1.path(), "alice")
        .args(["claim", &id1])
        .output()
        .unwrap();
    assert!(out1.status.success(), "stderr: {}", String::from_utf8_lossy(&out1.stderr));
    let s1 = String::from_utf8_lossy(&out1.stderr).to_string();
    assert!(notice_hit(&s1), "expected reactive notice, got: {s1}");

    // (2) --no-sync wins; sync doesn't fire, no notice.
    let (_r2, alice2, _bob2) = three_way();
    flip_repo_policy_on(alice2.path());
    let id2 = create_task(alice2.path(), "no-sync-flag");
    let out2 = bl_as(alice2.path(), "alice")
        .args(["claim", &id2, "--no-sync"])
        .output()
        .unwrap();
    assert!(out2.status.success());
    let s2 = String::from_utf8_lossy(&out2.stderr).to_string();
    assert!(!notice_hit(&s2), "expected no notice with --no-sync, got: {s2}");

    // (3) Clone-level override case: covered today only through XDG
    // (`~/.config/balls/<nested>/clone.json`, SPEC §6.4). The legacy
    // `.balls/local/config.json` reader retired with bl-5a03; the
    // XDG end-to-end coverage will follow the Phase 1B-2 cmd_init
    // flip (bl-a684). Until then the assertion is that the notice
    // still appears when the local-override layer is absent —
    // already covered by case (1).
}
