//! bl-733e — `bl close --delivered-repo <url>` lets the operator
//! override the auto-tagged delivery provenance. The auto-tag
//! ("the current clone's `origin`") is the right default but lies
//! when close is run on behalf of a different repo — typically a
//! bridge clone running close from a forge-sync hook (README
//! §Bridging to an external tracker).

mod common;

use common::*;
use common::forge;
use std::fs;

/// `--delivered <sha> --delivered-repo <url>` writes both fields
/// verbatim — the operator's declared source repo wins over the
/// local clone's `origin` auto-tag.
#[test]
fn close_delivered_repo_overrides_auto_tag_with_manual_sha() {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    forge::seed(alice.path(), Some("main"));
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["push", "origin", "main"]);

    let id = create_task(alice.path(), "feature");
    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(alice.path(), &id);
    fs::write(wt.join("f.txt"), "x").unwrap();
    bl(alice.path())
        .args(["review", &id, "-m", "Ship it"])
        .assert()
        .success();

    git(
        alice.path(),
        &[
            "commit",
            "--allow-empty",
            "-m",
            &format!("Ship it [{id}]"),
        ],
    );
    let merge_sha = git(alice.path(), &["rev-parse", "HEAD"]).trim().to_string();

    let child = forge::gate_child(alice.path());
    bl(alice.path())
        .args(["update", &child, "status=closed", "--note", "merged"])
        .assert()
        .success();

    let declared = "git@github.com:org/client-a.git";
    bl(alice.path())
        .args([
            "close",
            &id,
            "-m",
            "approved",
            "--delivered",
            &merge_sha,
            "--delivered-repo",
            declared,
        ])
        .assert()
        .success();

    let j = show_json(alice.path(), &id);
    assert_eq!(
        j["task"]["delivered_in"].as_str(),
        Some(merge_sha.as_str()),
        "manual sha is written verbatim"
    );
    assert_eq!(
        j["task"]["delivered_repo"].as_str(),
        Some(declared),
        "operator override beats the local-origin auto-tag"
    );
}

/// `--delivered-repo` alone (no `--delivered`) corrects the source
/// repo of an already-set `delivered_in` — the bridge-clone case
/// where local-squash review wrote the wrong auto-tag.
#[test]
fn close_delivered_repo_alone_corrects_existing_provenance() {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["push", "origin", "main"]);

    let id = create_task(alice.path(), "drifted");
    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(alice.path(), &id);
    fs::write(wt.join("f.txt"), "x").unwrap();
    bl(alice.path())
        .args(["review", &id, "-m", "ready"])
        .assert()
        .success();

    // Capture the auto-tagged sha + auto-tagged repo to confirm
    // they're present *before* the override.
    let before = show_json(alice.path(), &id);
    let sha = before["task"]["delivered_in"]
        .as_str()
        .expect("review tags delivered_in")
        .to_string();
    let auto_tag = remote.path().to_string_lossy().to_string();
    assert_eq!(
        before["task"]["delivered_repo"].as_str(),
        Some(auto_tag.as_str()),
        "local-squash review auto-tags origin"
    );

    let declared = "git@github.com:org/client-b.git";
    bl(alice.path())
        .args([
            "close",
            &id,
            "-m",
            "ok",
            "--delivered-repo",
            declared,
        ])
        .assert()
        .success();

    let j = show_json(alice.path(), &id);
    assert_eq!(
        j["task"]["delivered_in"].as_str(),
        Some(sha.as_str()),
        "sha must not change when only --delivered-repo is set"
    );
    assert_eq!(
        j["task"]["delivered_repo"].as_str(),
        Some(declared),
        "operator override replaces the auto-tag"
    );
}
