//! bl-499b — every task records which code repo created it. The
//! `repo` field rides the forward-compat extra seam, so it is absent
//! (no churn) on a single-repo store and legible on a shared hub.

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
fn provenance_falls_back_to_repo_dir_when_no_origin() {
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    let id = create_task(repo.path(), "no origin");

    let base = repo
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let j = show_json(repo.path(), &id);
    assert_eq!(j["task"]["repo"].as_str(), Some(base.as_str()));
}

#[test]
fn provenance_is_in_the_list_json_contract() {
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    let id = create_task(repo.path(), "listed");

    let out = bl(repo.path())
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

    let path = repo.path().join(".balls/tasks").join(format!("{id}.json"));
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v.as_object_mut().unwrap().remove("repo");
    std::fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

    let j = show_json(repo.path(), &id);
    assert!(j["task"]["repo"].is_null(), "missing repo defaults to null");
}
