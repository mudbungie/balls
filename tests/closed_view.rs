//! Closed/archived task visibility: `bl show <id>` history fallback
//! and `bl list --closed` / `--all`. Closed tasks are git-rm'd from
//! the state branch HEAD, so every assertion here exercises the
//! reconstruct-from-`balls/tasks`-history path.

mod common;

use common::*;
use predicates::prelude::*;

/// Create, claim, and close a task so only its state-branch history
/// remains. Returns the archived id.
fn closed_task(repo: &std::path::Path, title: &str) -> String {
    let id = create_task(repo, title);
    bl_as(repo, "alice").args(["claim", &id]).assert().success();
    bl_as(repo, "alice")
        .args(["close", &id, "-m", "done"])
        .assert()
        .success();
    assert!(!discover_tasks_dir(repo).join(format!("{id}.json")).exists());
    id
}

#[test]
fn show_reconstructs_closed_task_from_history() {
    let repo = new_repo();
    init_in(repo.path());
    let id = closed_task(repo.path(), "archived work");

    bl(repo.path())
        .args(["show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains(&id))
        .stdout(predicate::str::contains("archived work"))
        .stdout(predicate::str::contains("closed"));

    let out = bl(repo.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(v["task"]["status"], "closed");
    assert_eq!(v["task"]["id"], id);
}

#[test]
fn show_unknown_id_and_bad_id_still_error_cleanly() {
    let repo = new_repo();
    init_in(repo.path());
    // Never existed: load miss + recovery miss -> not found.
    bl(repo.path())
        .args(["show", "bl-1234"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
    // Malformed id: the non-not-found error arm.
    bl(repo.path())
        .args(["show", "bl-zz"])
        .assert()
        .failure();
}

#[test]
fn list_closed_only_lists_archived_tasks() {
    let repo = new_repo();
    init_in(repo.path());
    let a = closed_task(repo.path(), "first");
    let b = closed_task(repo.path(), "second");

    let out = bl(repo.path()).args(["list", "--closed"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains(&a) && s.contains(&b), "both closed listed: {s}");

    // Default list never reaches into history.
    let out = bl(repo.path()).arg("list").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(!s.contains(&a) && !s.contains(&b));

    // JSON keeps the bare-array shape, every entry status=closed.
    let out = bl(repo.path())
        .args(["list", "--closed", "--json"])
        .output()
        .unwrap();
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().all(|t| t["status"] == "closed"));
}

#[test]
fn list_status_closed_is_an_alias_for_closed() {
    let repo = new_repo();
    init_in(repo.path());
    let a = closed_task(repo.path(), "archived");
    bl(repo.path())
        .args(["list", "--status", "closed"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&a));
}

#[test]
fn list_all_combines_open_and_closed() {
    let repo = new_repo();
    init_in(repo.path());
    let open = create_task(repo.path(), "still open");
    let done = closed_task(repo.path(), "all done");

    let out = bl(repo.path()).args(["list", "--all"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains(&open) && s.contains(&done), "both shown: {s}");

    let out = bl(repo.path()).arg("list").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains(&open) && !s.contains(&done));
}

#[test]
fn stealth_store_reports_recovery_unavailable() {
    let repo = new_repo();
    bl(repo.path()).args(["init", "--stealth"]).assert().success();

    let note = "closed tasks live only in the git state branch";
    bl(repo.path())
        .args(["list", "--closed"])
        .assert()
        .success()
        .stderr(predicate::str::contains(note));
    bl(repo.path())
        .args(["list", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains(note));

    // Invalid status still errors (the parse `?` arm).
    bl(repo.path())
        .args(["list", "--status", "nope"])
        .assert()
        .failure();
}
