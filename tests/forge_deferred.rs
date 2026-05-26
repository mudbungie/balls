//! bl-b017 — deferred-squash review mode (SPEC §7.2, conformance §14
//! items 4–6 + the §5 explicit-target rule).
//!
//! Deferred mode: `bl review` pushes the work branch to the code
//! remote, opens a `forge-gate` child, links the parent `gates → child`
//! and flips it to `review` — without squashing into `target_branch`
//! or setting `delivered_in`. The existing gates close-blocker then
//! holds `bl close` until the gate child is closed (the BC hinge —
//! no old-client code path needed, conformance §9/§14.5).

mod common;

use common::forge::{gate_child, seed, sha};
use common::*;
use predicates::prelude::*;
use std::fs;

/// Conformance §14.4: deferred review pushes the work branch, opens the
/// gate, flips to review, and leaves the integration branch and
/// `delivered_in` untouched.
#[test]
fn deferred_review_pushes_branch_and_opens_gate() {
    let code = new_bare_remote();
    let alice = clone_from_remote(code.path(), "alice");
    seed(alice.path(), Some("main"));
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["push", "origin", "main"]);

    let main_before = sha(code.path(), "main");
    let id = create_task(alice.path(), "feature");
    bl(alice.path()).args(["claim", &id]).assert().success();
    let wt = worktree_path(alice.path(), &id);
    fs::write(wt.join("feature.txt"), "work").unwrap();

    bl(alice.path())
        .args(["review", &id, "-m", "Ship the feature"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("PR title: Ship the feature [{id}]")))
        .stdout(predicate::str::contains("pushed work/"));

    // Work branch is on the code remote.
    assert!(
        git_ok(
            code.path(),
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/work/{id}")],
        ),
        "work branch must be pushed to origin"
    );
    // Integration branch untouched, locally and on the remote.
    assert_eq!(sha(code.path(), "main"), main_before, "remote main untouched");
    assert!(
        !alice.path().join("feature.txt").exists(),
        "no local squash: main work tree untouched"
    );

    let j = read_task_json(alice.path(), &id);
    assert_eq!(j["status"], "review");
    assert!(j["delivered_in"].is_null(), "deferred mode leaves delivered_in null");

    let child = gate_child(alice.path());
    let cj = read_task_json(alice.path(), &child);
    assert_eq!(cj["parent"], id);
    assert_eq!(cj["status"], "open");
    assert_eq!(cj["title"], format!("Forge: PR merged for {id}"));
    let links = j["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["link_type"], "gates");
    assert_eq!(links[0]["target"], child);
}

/// Conformance §14.5 (the BC hinge): `bl close` on the parent before
/// the gate child closes is refused by the existing gates check, and
/// the worktree is left intact for resumption.
#[test]
fn close_blocked_until_gate_child_closes() {
    let code = new_bare_remote();
    let alice = clone_from_remote(code.path(), "alice");
    seed(alice.path(), Some("main"));
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["push", "origin", "main"]);

    let id = create_task(alice.path(), "feature");
    bl(alice.path()).args(["claim", &id]).assert().success();
    let wt = worktree_path(alice.path(), &id);
    fs::write(wt.join("f.txt"), "w").unwrap();
    bl(alice.path()).args(["review", &id, "-m", "go"]).assert().success();
    let child = gate_child(alice.path());

    // §14.5: refused, worktree intact.
    bl(alice.path())
        .args(["close", &id, "-m", "done"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("blocked by open gate"))
        .stderr(predicate::str::contains(&child));
    assert!(wt.exists(), "worktree intact after a blocked close");

    // §14.6: close the gate child (the forge merged the PR), then the
    // parent close succeeds and the worktree is torn down.
    bl(alice.path())
        .args(["update", &child, "status=closed", "--note", "merged in PR"])
        .assert()
        .success();
    bl(alice.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();
    assert!(
        !alice.path().join(format!(".balls/tasks/{id}.json")).exists(),
        "parent archived after the gate cleared"
    );
    assert!(!wt.exists(), "worktree removed on close");
}

/// SPEC §5: deferred mode rejects an implicit integration target. The
/// review fails before any mutation — status stays `in_progress`, no
/// gate child is created.
#[test]
fn deferred_review_requires_explicit_target_branch() {
    let repo = new_repo();
    seed(repo.path(), None); // delivery=deferred, target_branch unset
    init_in(repo.path());

    let id = create_task(repo.path(), "feature");
    bl(repo.path()).args(["claim", &id]).assert().success();
    let wt = worktree_path(repo.path(), &id);
    fs::write(wt.join("f.txt"), "w").unwrap();

    bl(repo.path())
        .args(["review", &id, "-m", "go"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires an explicit target_branch"));

    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "in_progress", "no flip on a rejected review");
    let listed = bl(repo.path())
        .args(["list", "--json", "--tag", "forge-gate"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 0, "no gate child on failure");
}
