//! bl-4fbc — end-to-end coverage for `bl close --resolve-remote`.
//!
//! bl-e454 shipped `--resolve-remote` (and the deferred-mode auto-
//! engage) with five unit tests, all driving `delivery::
//! populate_on_close` directly. The CLI wiring between the flag and
//! that function — `CloseArgs.resolve_remote` → `cmd_close`'s
//! `resolve_remote || deferred` → `close_worktree` → `populate_on_close`
//! — had no end-to-end test. 100% line coverage does not catch a
//! regression there: the `resolve_remote || deferred` expression is
//! marked covered by the flag-*false* close tests, so a dropped
//! argument or an inverted bool would pass CI silently.
//!
//! These tests drive the real `bl` binary across two repos: a task
//! store whose integration branch never carries the `[bl-xxxx]`
//! squash, and a separate code repo that does. Only the flag — or
//! deferred mode — can bridge that gap.
//!
//! Primary assertions are on `delivered_in`. The `delivered_repo`
//! value a remote-resolved close writes is wrong today and is the
//! subject of bl-6816; it is deliberately not asserted here so this
//! suite stays stable whichever way that ball lands.

mod common;

use common::forge::{gate_child, seed};
use common::*;
use std::fs;
use std::path::Path;

fn show_json(repo: &Path, id: &str) -> serde_json::Value {
    let out = bl(repo).args(["show", id, "--json"]).output().unwrap();
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).unwrap()
}

/// A standalone code repo whose history carries one `[id]`-tagged
/// commit — the squash a forge merge would land. A plain git repo
/// suffices: `delivery_remote::resolve` bare-clones it and scans
/// every ref for the tag. Returns `(repo, delivered_sha)`.
fn code_repo_with_delivery(id: &str) -> (Repo, String) {
    let code = new_repo();
    git(
        code.path(),
        &[
            "commit",
            "--allow-empty",
            "-qm",
            &format!("implement feature [{id}]"),
        ],
    );
    let sha = git(code.path(), &["rev-parse", "HEAD"]).trim().to_string();
    (code, sha)
}

/// Flip `id` into a closeable cross-repo `review` state: status
/// `review`, `delivered_repo` pointing at `code`, `delivered_in` left
/// null. The edit is committed on the state branch (the
/// `.balls/state-repo` checkout of `balls/tasks`) so a later `bl show`
/// on the archived task reconstructs the injected provenance from
/// history, not the `bl create` original.
fn link_delivery(repo: &Path, id: &str, code: &Path) {
    let file = repo.join(".balls/tasks").join(format!("{id}.json"));
    let mut task: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    task["status"] = serde_json::Value::String("review".into());
    task["delivered_repo"] =
        serde_json::Value::String(code.to_string_lossy().into_owned());
    fs::write(&file, serde_json::to_string(&task).unwrap()).unwrap();

    let state = repo.join(".balls/state-repo");
    git(&state, &["add", &format!(".balls/tasks/{id}.json")]);
    git(
        &state,
        &["commit", "-qm", &format!("fixture: {id} reviewed cross-repo")],
    );
}

/// Build the bl-4fbc scenario: a `closer` task store holding `id` at
/// `review`, its code delivered into a *separate* `code` repo the
/// closer's own integration branch never sees. Returns
/// `(code, closer, id, delivered_sha)`.
fn cross_repo_closer() -> (Repo, Repo, String, String) {
    let closer = new_repo();
    init_in(closer.path());
    let id = create_task(closer.path(), "cross-repo feature");
    let (code, sha) = code_repo_with_delivery(&id);
    link_delivery(closer.path(), &id, code.path());
    (code, closer, id, sha)
}

/// End-to-end: `bl close --resolve-remote` threads the flag through
/// the full CLI wiring and the archived task carries the sha
/// recovered from `delivered_repo` — the local integration branch
/// never had it.
#[test]
fn close_resolve_remote_populates_delivered_in() {
    let (_code, closer, id, sha) = cross_repo_closer();

    bl(closer.path())
        .args(["close", &id, "-m", "approved", "--resolve-remote"])
        .assert()
        .success();

    let j = show_json(closer.path(), &id);
    assert_eq!(
        j["task"]["delivered_in"].as_str(),
        Some(sha.as_str()),
        "the flag must thread through to a cross-repo-resolved sha",
    );
}

/// Companion proving the flag is load-bearing: the identical close
/// *without* `--resolve-remote` leaves `delivered_in` null. If the
/// CLI ever drops the argument or inverts the bool, this is the test
/// that fails — line coverage of `resolve_remote || deferred` never
/// would.
#[test]
fn close_without_resolve_remote_leaves_delivered_in_null() {
    let (_code, closer, id, _sha) = cross_repo_closer();

    bl(closer.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();

    let j = show_json(closer.path(), &id);
    assert!(
        j["task"]["delivered_in"].is_null(),
        "no flag, no remote resolution — delivered_in stays null",
    );
}

/// Deferred mode auto-engages resolution: `cmd_close` ORs the
/// `--resolve-remote` flag with the deferred-delivery flag, so a
/// bridge running `bl close` from a forge-sync hook resolves
/// `delivered_in` with no flag on the command line. Covers the
/// `|| deferred` operand the explicit-flag tests leave dark.
#[test]
fn deferred_close_auto_resolves_without_flag() {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    seed(alice.path(), Some("main")); // delivery.mode = deferred
    bl(alice.path()).arg("init").assert().success();
    git(alice.path(), &["push", "origin", "main"]);

    let id = create_task(alice.path(), "feature");
    bl(alice.path()).args(["claim", &id]).assert().success();
    let wt = alice.path().join(".balls-worktrees").join(&id);
    fs::write(wt.join("f.txt"), "w").unwrap();
    bl(alice.path())
        .args(["review", &id, "-m", "open the PR"])
        .assert()
        .success();

    // The forge merged the PR into a separate code repo; the gate
    // child closes (a forge-sync hook would do this unattended).
    let (code, merge_sha) = code_repo_with_delivery(&id);
    link_delivery(alice.path(), &id, code.path());
    let child = gate_child(alice.path());
    bl(alice.path())
        .args(["update", &child, "status=closed", "--note", "merged"])
        .assert()
        .success();

    // No --resolve-remote on the command line: deferred mode engages
    // cross-repo resolution itself.
    bl(alice.path())
        .args(["close", &id, "-m", "approved"])
        .assert()
        .success();

    let j = show_json(alice.path(), &id);
    assert_eq!(
        j["task"]["delivered_in"].as_str(),
        Some(merge_sha.as_str()),
        "deferred close resolves delivered_in with no flag",
    );
}
