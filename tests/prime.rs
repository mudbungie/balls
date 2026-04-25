//! `bl prime` rendering and per-claim status indicators (main_ahead,
//! overlap_files). Exercises the parallel-agent collision signals
//! introduced after the lernie postmortem (2026-04-22).

mod common;

use common::*;

#[test]
fn prime_no_claimed_tasks() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "just one");
    let out = bl_as(repo.path(), "nobody")
        .arg("prime")
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("nobody"));
    assert!(s.contains("just one"));
}

#[test]
fn prime_json_shape() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "one");
    let out = bl_as(repo.path(), "agent")
        .args(["prime", "--json"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    // Stdout must be a single JSON document so `bl prime --json | jq` works.
    let v: serde_json::Value =
        serde_json::from_str(s.trim()).expect("stdout must be pure JSON");
    assert_eq!(v["identity"], "agent");
    assert!(v["ready"].is_array());
    assert!(v["claimed"].is_array());
}

#[test]
fn prime_text_output_with_claimed_task() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "work in progress");
    bl_as(repo.path(), "agent-a")
        .args(["claim", &id])
        .assert()
        .success();
    let out = bl_as(repo.path(), "agent-a")
        .arg("prime")
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("Claimed (resume)"));
    assert!(s.contains("work in progress"));
    // No suffix when main hasn't moved past the claim point.
    assert!(!s.contains("main +"));
}

#[test]
fn prime_warns_when_main_advanced_since_claim() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "stale base");
    bl_as(repo.path(), "agent-a")
        .args(["claim", &id])
        .assert()
        .success();

    std::fs::write(repo.path().join("a.txt"), "a").unwrap();
    git(repo.path(), &["add", "a.txt"]);
    git(repo.path(), &["commit", "-m", "advance main", "--no-verify"]);
    std::fs::write(repo.path().join("b.txt"), "b").unwrap();
    git(repo.path(), &["add", "b.txt"]);
    git(repo.path(), &["commit", "-m", "advance again", "--no-verify"]);

    let out = bl_as(repo.path(), "agent-a")
        .arg("prime")
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        s.contains("main +2 since claim"),
        "expected 'main +2 since claim' in:\n{s}"
    );
}

#[test]
fn prime_json_stdout_has_no_leakage() {
    // Regression: cmd_sync used to println!("sync complete"), corrupting
    // stdout for `bl prime --json | jq` consumers. The line belongs on
    // stderr.
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "one");
    let out = bl_as(repo.path(), "agent")
        .args(["prime", "--json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        !stdout.contains("sync complete"),
        "'sync complete' must not appear on stdout:\n{stdout}"
    );
    serde_json::from_str::<serde_json::Value>(stdout.trim())
        .expect("stdout must be pure JSON");
}

#[test]
fn prime_json_includes_claimed_status() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "with status");
    bl_as(repo.path(), "agent-a")
        .args(["claim", &id])
        .assert()
        .success();

    std::fs::write(repo.path().join("c.txt"), "c").unwrap();
    git(repo.path(), &["add", "c.txt"]);
    git(repo.path(), &["commit", "-m", "advance", "--no-verify"]);

    let out = bl_as(repo.path(), "agent-a")
        .args(["prime", "--json"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    let status = v["claimed_status"].as_array().expect("claimed_status array");
    assert_eq!(status.len(), 1);
    assert_eq!(status[0]["id"], id);
    assert_eq!(status[0]["main_ahead"], 1);
    assert_eq!(status[0]["overlap_files"].as_array().unwrap().len(), 0);
}

#[test]
fn prime_skips_indicators_for_no_worktree_claim() {
    // A --no-worktree claim has no work branch, so the main-ahead and
    // overlap signals are unknowable. Prime renders zeros silently.
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "no worktree claim");
    bl_as(repo.path(), "agent-a")
        .args(["claim", &id, "--no-worktree"])
        .assert()
        .success();

    std::fs::write(repo.path().join("z.txt"), "z").unwrap();
    git(repo.path(), &["add", "z.txt"]);
    git(repo.path(), &["commit", "-m", "advance", "--no-verify"]);

    let out = bl_as(repo.path(), "agent-a")
        .args(["prime", "--json"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(s.trim()).unwrap();
    let status = &v["claimed_status"][0];
    assert_eq!(status["main_ahead"], 0);
    assert_eq!(status["overlap_files"].as_array().unwrap().len(), 0);
}

#[test]
fn prime_warns_when_main_overlaps_work_files() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "overlap risk");
    bl_as(repo.path(), "agent-a")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = repo.path().join(".balls-worktrees").join(&id);

    std::fs::write(wt.join("shared.txt"), "agent edit").unwrap();
    git(&wt, &["add", "shared.txt"]);
    git(&wt, &["commit", "-m", "agent change", "--no-verify"]);

    std::fs::write(repo.path().join("shared.txt"), "main edit").unwrap();
    git(repo.path(), &["add", "shared.txt"]);
    git(repo.path(), &["commit", "-m", "main change", "--no-verify"]);

    let out = bl_as(repo.path(), "agent-a")
        .arg("prime")
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        s.contains("main +1 since claim, 1 overlap"),
        "expected combined ahead+overlap suffix in:\n{s}"
    );

    let out = bl_as(repo.path(), "agent-a")
        .args(["prime", "--json"])
        .output()
        .unwrap();
    let j = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(j.trim()).unwrap();
    let files = v["claimed_status"][0]["overlap_files"]
        .as_array()
        .expect("overlap_files array");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], "shared.txt");
}
