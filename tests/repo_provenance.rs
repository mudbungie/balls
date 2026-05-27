//! bl-499b / bl-8994 — every task records which code repo its work
//! belongs to. `repo` is stamped at `bl create` (the creating clone's
//! `origin`) and re-anchored at `bl claim` to the claiming clone, the
//! definitive code home. Only a fetchable URL is ever auto-written —
//! a clone with no `origin` leaves `repo` null rather than persist a
//! bare basename no sibling clone can fetch.

mod common;

use common::*;
use predicates::prelude::*;

fn show_json(repo_root: &std::path::Path, id: &str) -> serde_json::Value {
    let out = bl(repo_root)
        .args(["show", id, "--json"])
        .output()
        .expect("bl show");
    assert!(out.status.success());
    serde_json::from_slice(&out.stdout).expect("json")
}

fn repo_field(repo_root: &std::path::Path, id: &str) -> serde_json::Value {
    show_json(repo_root, id)["task"]["repo"].clone()
}

#[test]
fn provenance_is_the_origin_url_when_there_is_one() {
    let remote = new_bare_remote();
    let dev = clone_from_remote(remote.path(), "dev");
    bl(dev.path()).arg("init").assert().success();
    let id = create_task(dev.path(), "with origin");

    let want = remote.path().to_string_lossy().to_string();
    let j = show_json(dev.path(), &id);
    assert_eq!(j["task"]["repo"].as_str(), Some(want.as_str()));

    // Human `bl show` surfaces it too.
    bl(dev.path())
        .args(["show", &id])
        .assert()
        .success()
        .stdout(predicate::str::contains("repo:"))
        .stdout(predicate::str::contains(want.as_str()));
}

#[test]
fn provenance_is_null_at_create_without_an_origin() {
    // `bl create` only knows where the ball was filed. With no
    // `origin` it cannot name a fetchable code repo, so it writes
    // null rather than a bare basename — `bl claim` anchors it later.
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    let id = create_task(repo.path(), "no origin");
    assert!(repo_field(repo.path(), &id).is_null());
}

#[test]
fn provenance_is_in_the_list_json_contract() {
    // A task whose clone has an `origin` carries `repo` in the
    // `bl list --json` contract.
    let remote = new_bare_remote();
    let dev = clone_from_remote(remote.path(), "dev");
    bl(dev.path()).arg("init").assert().success();
    let id = create_task(dev.path(), "listed");

    let out = bl(dev.path())
        .args(["list", "--json"])
        .output()
        .expect("bl list");
    let arr: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let t = arr
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == id)
        .expect("task in list");
    assert!(t["repo"].is_string(), "repo present in list --json: {t}");
}

#[test]
fn legacy_task_file_without_repo_still_loads() {
    // A task file written before this field existed has no `repo`
    // key; it must load and show fine (serde default → null).
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    let id = create_task(repo.path(), "legacy");

    let path = discover_tasks_dir(repo.path()).join(format!("{id}.json"));
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v.as_object_mut().unwrap().remove("repo");
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

    assert!(
        repo_field(repo.path(), &id).is_null(),
        "missing repo defaults to null",
    );
}

#[test]
fn claim_anchors_repo_when_create_left_it_null() {
    // `bl create` in a clone with no `origin` leaves `repo` null.
    // `bl claim` is the authoritative anchor point: once the clone
    // has an `origin`, claiming stamps it.
    let remote = new_bare_remote();
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    let id = create_task(repo.path(), "anchored at claim");
    assert!(repo_field(repo.path(), &id).is_null());

    let remote_str = remote.path().to_string_lossy().to_string();
    git(repo.path(), &["remote", "add", "origin", &remote_str]);
    bl(repo.path()).args(["claim", &id]).assert().success();

    assert_eq!(repo_field(repo.path(), &id).as_str(), Some(remote_str.as_str()));
}

#[test]
fn claim_leaves_repo_null_when_the_clone_has_no_origin() {
    // No `origin` anywhere in the task's life: claim must not write a
    // bare basename — an unfetchable string is worse than null.
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    let id = create_task(repo.path(), "no origin ever");
    bl(repo.path()).args(["claim", &id]).assert().success();
    assert!(
        repo_field(repo.path(), &id).is_null(),
        "claim must not stamp a basename",
    );
}

#[test]
fn claim_reanchors_repo_to_the_claiming_clone() {
    // A ball filed against one origin, then claimed from the clone
    // whose `origin` is the real code home: claim overwrites the
    // create-time guess. And nothing re-stamps it afterwards —
    // `bl review` is a lifecycle step and leaves the value intact.
    let filed = new_bare_remote();
    let code = new_bare_remote();
    let repo = new_repo();
    let filed_str = filed.path().to_string_lossy().to_string();
    git(repo.path(), &["remote", "add", "origin", &filed_str]);
    bl(repo.path()).arg("init").assert().success();
    let id = create_task(repo.path(), "re-anchored");
    assert_eq!(repo_field(repo.path(), &id).as_str(), Some(filed_str.as_str()));

    let code_str = code.path().to_string_lossy().to_string();
    git(repo.path(), &["remote", "set-url", "origin", &code_str]);
    bl(repo.path()).args(["claim", &id]).assert().success();
    assert_eq!(repo_field(repo.path(), &id).as_str(), Some(code_str.as_str()));

    // Freeze: `bl review` must not re-stamp repo.
    let wt = worktree_path(repo.path(), &id);
    std::fs::write(wt.join("f.txt"), "x").unwrap();
    bl(repo.path())
        .args(["review", &id, "-m", "go"])
        .assert()
        .success();
    assert_eq!(repo_field(repo.path(), &id).as_str(), Some(code_str.as_str()));
}

#[test]
fn update_can_explicitly_set_and_clear_repo() {
    // `repo` is implicitly frozen by the lifecycle, but never locked:
    // an explicit `bl update repo=` is the fixup path when a task is
    // reassigned to a different code repo.
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    let id = create_task(repo.path(), "manual repo fixup");

    bl(repo.path())
        .args(["update", &id, "repo=git@h:explicit.git"])
        .assert()
        .success();
    assert_eq!(
        repo_field(repo.path(), &id).as_str(),
        Some("git@h:explicit.git"),
    );

    // Both `repo=null` and a bare `repo=` clear it back to null.
    bl(repo.path())
        .args(["update", &id, "repo=null"])
        .assert()
        .success();
    assert!(repo_field(repo.path(), &id).is_null());

    bl(repo.path())
        .args(["update", &id, "repo=git@h:again.git"])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &id, "repo="])
        .assert()
        .success();
    assert!(repo_field(repo.path(), &id).is_null());
}
