//! bl-3cc2 — deferred-mode reject path (SPEC §7.3, conformance §14.8).
//!
//! Rejecting a deferred-mode review (`bl update <parent>
//! status=in_progress`) must additionally close the auto-opened
//! `forge-gate` child in the SAME state-branch commit — keeping the
//! invariant that a task is `in_progress` iff it has no open gate
//! child — or refuse the update atomically if the child cannot be
//! closed cleanly. The work branch on origin is left alone.

mod common;

use common::forge::{gate_child, seed};
use common::*;
use predicates::prelude::*;
use std::fs;

/// Conformance §14.8 (SPEC §7.3): rejecting a deferred-mode review with
/// `bl update <parent> status=in_progress` closes the forge-gate child
/// atomically in the SAME state-branch commit, drops the dead `gates`
/// link, flips the parent to `in_progress`, records the note, and
/// leaves the work branch on origin alone.
#[test]
fn deferred_reject_closes_gate_child_atomically() {
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

    bl(alice.path())
        .args(["update", &id, "status=in_progress", "--note", "needs rework"])
        .assert()
        .success();

    // Parent: in_progress, dead gates link dropped, child recorded.
    let j = read_task_json(alice.path(), &id);
    assert_eq!(j["status"], "in_progress");
    assert!(
        j["links"].as_array().unwrap().is_empty(),
        "the dead gates link is dropped on reject"
    );
    let closed: Vec<String> = j["closed_children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(closed, vec![child.clone()], "gate child archived under parent");
    let notes = read_task_notes(alice.path(), &id);
    assert!(
        notes.iter().any(|n| n["text"] == "needs rework"),
        "reject note recorded on the parent: {notes:?}"
    );

    // Gate child is archived (file gone), not just flipped.
    assert!(
        !discover_tasks_dir(alice.path()).join(format!("{child}.json")).exists(),
        "gate child file removed on reject"
    );
    let listed = bl(alice.path())
        .args(["list", "--json", "--tag", "forge-gate"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 0, "no open forge-gate child remains");

    // Atomic: the parent flip and the child removal are ONE commit.
    let state_wt = discover_state_repo(alice.path()).expect("non-stealth state checkout");
    let head = git(&state_wt, &["rev-parse", "HEAD"]);
    let head = head.trim();
    let names = git(&state_wt, &["show", "--name-status", "--format=%s", "HEAD"]);
    assert!(
        names.contains(&format!("reject {id} deferred")),
        "HEAD is the reject commit: {names}"
    );
    assert!(
        names.contains(&format!(".balls/tasks/{id}.json"))
            && names.contains(&format!(".balls/tasks/{child}.json")),
        "one commit touches both parent and gate child: {names}"
    );
    let child_last = git(
        &state_wt,
        &["log", "-1", "--format=%H", "--", &format!(".balls/tasks/{child}.json")],
    );
    assert_eq!(
        child_last.trim(),
        head,
        "gate child archived in the same commit as the parent flip"
    );

    // SPEC §7.3: work branch on origin left alone; worktree preserved.
    assert!(
        git_ok(
            code.path(),
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/work/{id}")],
        ),
        "work branch on origin untouched by reject"
    );
    assert!(wt.exists(), "claimant worktree preserved across reject");
}

/// SPEC §7.3: the reject is all-or-nothing. If the forge-gate child
/// cannot be closed cleanly — here an agent claimed it against the
/// SKILL guidance — `bl update <parent> status=in_progress` is refused
/// with NO mutation to either task.
#[test]
fn deferred_reject_refused_when_gate_child_claimed() {
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

    // Against SKILL guidance, an agent claims the gate child.
    bl(alice.path()).args(["claim", &child]).assert().success();

    bl(alice.path())
        .args(["update", &id, "status=in_progress", "--note", "rework"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "gate child {child} is claimed"
        )));

    // No mutation: parent still review + gated, child still open.
    let j = read_task_json(alice.path(), &id);
    assert_eq!(j["status"], "review", "parent untouched on refusal");
    let links = j["links"].as_array().unwrap();
    assert_eq!(links.len(), 1, "gates link intact");
    assert_eq!(links[0]["target"], child);
    let cj = read_task_json(alice.path(), &child);
    assert_eq!(cj["status"], "in_progress", "gate child not closed");
    let notes = read_task_notes(alice.path(), &id);
    assert!(
        !notes.iter().any(|n| n["text"] == "rework"),
        "no note written on a refused reject: {notes:?}"
    );
}
