//! bl-87ea — `bl close` resolves `delivered_in` via tag-scan when null
//! (SPEC §6). Deferred-mode `bl review` never lands a local squash, so
//! it never writes the `delivered_in` hint. By the time `bl close`
//! runs after the forge merges the PR, the field is still null; close
//! tag-scans the effective target branch for the `[bl-xxxx]` commit
//! and caches it into the archived task, or warns and proceeds when
//! there is no match. `--delivered <sha>` overrides the scan outright.

mod common;

use common::*;
use common::forge;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

/// Raw stored `delivered_in` of a (possibly archived) task, read back
/// through `bl show --json` which reconstructs from the state branch.
fn stored_delivered_in(repo: &Path, id: &str) -> serde_json::Value {
    show_json(repo, id)["task"]["delivered_in"].clone()
}

/// Drive a task through deferred review and return its id plus the
/// alice workspace; leaves the parent in `review`, gated by its child.
fn deferred_reviewed() -> (Repo, Repo, String) {
    let code = new_bare_remote();
    let alice = clone_from_remote(code.path(), "alice");
    forge::seed(alice.path(), Some("main"));
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["push", "origin", "main"]);

    let id = create_task(alice.path(), "feature");
    bl(alice.path()).args(["claim", &id]).assert().success();
    let wt = alice.path().join(".balls-worktrees").join(&id);
    fs::write(wt.join("feature.txt"), "work").unwrap();
    bl(alice.path())
        .args(["review", &id, "-m", "Ship it"])
        .assert()
        .success();
    (code, alice, id)
}

/// The forge merged the PR: a `[id]`-tagged squash now sits on the
/// integration branch. `bl close` tag-scans it and caches the SHA
/// into the archived task even though `review` never wrote the hint.
#[test]
fn deferred_close_auto_populates_delivered_in() {
    let (_code, alice, id) = deferred_reviewed();
    git(
        alice.path(),
        &["commit", "--allow-empty", "-m", &format!("Ship it [{id}]")],
    );
    let merge_sha = git(alice.path(), &["rev-parse", "HEAD"]).trim().to_string();

    let child = forge::gate_child(alice.path());
    bl(alice.path())
        .args(["update", &child, "status=closed", "--note", "merged"])
        .assert()
        .success();
    bl(alice.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();

    assert_eq!(
        stored_delivered_in(alice.path(), &id).as_str(),
        Some(merge_sha.as_str()),
        "close tag-scans the target branch and caches the merge SHA"
    );
}

/// No `[id]` commit on the target branch (tag missing from the PR
/// title, or the merge SHA not fetched yet): close warns and proceeds,
/// leaving `delivered_in` null — the tag stays ground truth.
#[test]
fn deferred_close_scan_miss_warns_and_proceeds() {
    let (_code, alice, id) = deferred_reviewed();
    let child = forge::gate_child(alice.path());
    bl(alice.path())
        .args(["update", &child, "status=closed", "--note", "merged"])
        .assert()
        .success();
    bl(alice.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success()
        .stderr(predicate::str::contains("without delivered_in"));

    assert!(
        stored_delivered_in(alice.path(), &id).is_null(),
        "a scan miss leaves the hint null"
    );
    assert!(
        !alice.path().join(format!(".balls/tasks/{id}.json")).exists(),
        "close still proceeds on a scan miss"
    );
}

/// `--delivered <sha>` wins unconditionally: it is written verbatim
/// even though a *different* `[id]`-tagged commit is reachable and the
/// scan would otherwise resolve to it (forge rebase-merge case).
#[test]
fn close_delivered_flag_overrides_scan() {
    let (_code, alice, id) = deferred_reviewed();
    git(
        alice.path(),
        &["commit", "--allow-empty", "-m", "operator pick"],
    );
    let chosen = git(alice.path(), &["rev-parse", "HEAD"]).trim().to_string();
    git(
        alice.path(),
        &["commit", "--allow-empty", "-m", &format!("auto squash [{id}]")],
    );

    let child = forge::gate_child(alice.path());
    bl(alice.path())
        .args(["update", &child, "status=closed", "--note", "merged"])
        .assert()
        .success();
    bl(alice.path())
        .args(["close", &id, "-m", "approved", "--delivered", &chosen])
        .assert()
        .success();

    assert_eq!(
        stored_delivered_in(alice.path(), &id).as_str(),
        Some(chosen.as_str()),
        "the explicit --delivered SHA wins over the tag scan"
    );
}
