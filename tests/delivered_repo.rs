//! bl-7523 — `delivered_in` carries a `delivered_repo` provenance
//! string so a sha is resolvable cross-repo. Mirrors `repo_provenance`
//! for the *create*-time `repo` field; here the field is set by
//! `bl review` (local-squash) and by `bl close` (deferred / manual
//! `--delivered`). Pre-bl-7523 tasks lack the field; readers must
//! interpret a null as "the locally-checked-out repo," so legacy task
//! files still load and the single-repo case stays byte-identical.

mod common;

use common::*;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

fn show_json(repo: &Path, id: &str) -> serde_json::Value {
    let out = bl(repo).args(["show", id, "--json"]).output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}

fn claim_and_seed(repo: &Path, id: &str) {
    bl_as(repo, "alice").args(["claim", id]).assert().success();
    let wt = repo.join(".balls-worktrees").join(id);
    fs::write(wt.join("f.txt"), "x").unwrap();
}

/// `bl review` (local-squash) tags the delivery with the client's
/// origin URL — the same shape `task.repo` uses.
#[test]
fn local_squash_review_tags_delivered_repo_with_origin_url() {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    init_in(alice.path());
    let id = create_task(alice.path(), "deliverable");
    claim_and_seed(alice.path(), &id);
    bl(alice.path())
        .args(["review", &id, "-m", "go"])
        .assert()
        .success();

    let want = remote.path().to_string_lossy().to_string();
    let j = show_json(alice.path(), &id);
    assert_eq!(j["task"]["delivered_repo"].as_str(), Some(want.as_str()));
}

/// No `origin` configured: provenance falls back to the basename, the
/// same fallback chain `task.repo` uses (`balls::repo_url::current`).
#[test]
fn local_squash_review_falls_back_to_basename_without_origin() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "no-origin");
    claim_and_seed(repo.path(), &id);
    bl(repo.path())
        .args(["review", &id, "-m", "go"])
        .assert()
        .success();

    let base = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let j = show_json(repo.path(), &id);
    assert_eq!(j["task"]["delivered_repo"].as_str(), Some(base.as_str()));
}

/// `bl show` surfaces the field in its human render so a reader can
/// see "this sha lives in repo X" without re-running with --json.
#[test]
fn show_displays_delivered_repo_line() {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    init_in(alice.path());
    let id = create_task(alice.path(), "showable");
    claim_and_seed(alice.path(), &id);
    bl(alice.path())
        .args(["review", &id, "-m", "go"])
        .assert()
        .success();

    let want = remote.path().to_string_lossy().to_string();
    bl(alice.path())
        .args(["show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("delivered repo:"))
        .stdout(predicate::str::contains(want.as_str()));
}

/// A no-code "checkpoint" review (worker had nothing to commit) has
/// no squash sha to tag, so `delivered_repo` stays null — mirrors the
/// existing `delivered_in: null` contract for the same case.
#[test]
fn checkpoint_review_leaves_delivered_repo_null() {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    init_in(alice.path());
    let id = create_task(alice.path(), "checkpoint");
    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    // Deliberately *no* file write — bl review will produce no squash.
    bl(alice.path())
        .args(["review", &id, "-m", "no work yet"])
        .assert()
        .success();

    let j = show_json(alice.path(), &id);
    assert!(
        j["task"]["delivered_in"].is_null(),
        "checkpoint review must not write a sha: {}",
        j["task"]
    );
    assert!(
        j["task"]["delivered_repo"].is_null(),
        "no sha → no provenance: {}",
        j["task"]
    );
}

/// `bl close --delivered <sha>` is the deferred-mode operator override.
/// It writes `delivered_in` verbatim and also tags the current clone
/// as the delivery's source — the manual sha by definition lives in
/// the local checkout.
fn deferred_seed(repo: &Path) {
    let balls = repo.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    fs::write(
        balls.join("config.json"),
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees","target_branch":"main","delivery":{"mode":"deferred"}}"#,
    )
    .unwrap();
}

fn deferred_gate_child(repo: &Path) -> String {
    let out = bl(repo)
        .args(["list", "--json", "--tag", "forge-gate"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v.as_array().unwrap()[0]["id"].as_str().unwrap().to_string()
}

#[test]
fn deferred_close_tag_scan_tags_delivered_repo() {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    deferred_seed(alice.path());
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["push", "origin", "main"]);

    let id = create_task(alice.path(), "feature");
    claim_and_seed(alice.path(), &id);
    bl(alice.path())
        .args(["review", &id, "-m", "Ship it"])
        .assert()
        .success();

    // Simulate the forge merging the PR: an `[id]`-tagged commit on
    // main is what `bl close` will tag-scan and cache into the task.
    git(
        alice.path(),
        &[
            "commit",
            "--allow-empty",
            "-m",
            &format!("Ship it [{id}]"),
        ],
    );

    let child = deferred_gate_child(alice.path());
    bl(alice.path())
        .args(["update", &child, "status=closed", "--note", "merged"])
        .assert()
        .success();
    bl(alice.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();

    // Show reconstructs the archived task from the state branch.
    let j = show_json(alice.path(), &id);
    let want = remote.path().to_string_lossy().to_string();
    assert_eq!(
        j["task"]["delivered_repo"].as_str(),
        Some(want.as_str()),
        "deferred close should tag the local clone's origin: {}",
        j["task"]
    );
}

/// A pre-bl-7523 task file (no `delivered_repo` key) still loads and
/// `bl show` reports null — the agreed reading is "delivered against
/// the locally-checked-out repo," so no retrofit is needed.
#[test]
fn legacy_task_file_without_delivered_repo_still_loads() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "legacy");
    claim_and_seed(repo.path(), &id);
    bl(repo.path())
        .args(["review", &id, "-m", "go"])
        .assert()
        .success();

    // Strip `delivered_repo` from the task file to simulate state
    // written by a pre-bl-7523 `bl`.
    let path = repo.path().join(".balls/tasks").join(format!("{id}.json"));
    let raw = fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v.as_object_mut().unwrap().remove("delivered_repo");
    fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

    let j = show_json(repo.path(), &id);
    assert!(
        j["task"]["delivered_repo"].is_null(),
        "missing field defaults to null: {}",
        j["task"]
    );
}
