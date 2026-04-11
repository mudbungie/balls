//! Coverage tests for read-oriented commands: list, show, ready, prime.

mod common;

use common::*;

#[test]
fn list_json_output_shape() {
    let repo = new_repo();
    init_in(repo.path());
    create_task(repo.path(), "one");
    create_task(repo.path(), "two");
    let out = bl(repo.path()).args(["list", "--json"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 2);
}

#[test]
fn show_renders_all_optional_sections() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "parent");
    let dep = create_task(repo.path(), "dep-task");
    let id = create_task_full(repo.path(), "feature", 2, &[&dep], &["auth"]);
    bl(repo.path())
        .args([
            "update",
            &id,
            &format!("parent={}", parent),
            "description=My feature",
            "--note",
            "first note",
        ])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &dep, "status=closed"])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    bl(repo.path())
        .args(["create", "kid-a", "--parent", &parent])
        .assert()
        .success();
    bl(repo.path())
        .args(["create", "kid-b", "--parent", &parent])
        .assert()
        .success();

    let out = bl(repo.path()).args(["show", &parent]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("children:"));
    assert!(s.contains("completion:"));

    let out = bl(repo.path()).args(["show", &id]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("parent:"));
    assert!(s.contains("deps:"));
    assert!(s.contains("tags:"));
    assert!(s.contains("claimed:"));
    assert!(s.contains("branch:"));
    assert!(s.contains("My feature"));
    assert!(s.contains("notes:"));
}

#[test]
fn show_reports_dep_blocked() {
    let repo = new_repo();
    init_in(repo.path());
    let dep = create_task(repo.path(), "dep");
    let id = create_task_full(repo.path(), "blocked-task", 3, &[&dep], &[]);
    let out = bl(repo.path()).args(["show", &id]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("dep_blocked: yes"));
}

#[test]
fn ready_auto_fetch_hits_remote_path() {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    bl(alice.path()).arg("init").assert().success();
    create_task(alice.path(), "t");
    push(alice.path());

    let last_fetch = alice.path().join(".balls/local/last_fetch");
    let _ = std::fs::remove_file(&last_fetch);
    bl(alice.path()).arg("ready").assert().success();
    assert!(last_fetch.exists());

    let cfg_path = alice.path().join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["stale_threshold_seconds"] = serde_json::json!(0);
    std::fs::write(&cfg_path, cfg.to_string()).unwrap();
    bl(alice.path()).arg("ready").assert().success();
}

#[test]
fn list_when_tasks_dir_absent() {
    let repo = new_repo();
    init_in(repo.path());
    std::fs::remove_dir_all(repo.path().join(".balls/tasks")).unwrap();
    bl(repo.path()).arg("list").assert().success();
}

#[test]
fn list_with_malformed_task_warns_and_continues() {
    let repo = new_repo();
    init_in(repo.path());
    let ok_id = create_task(repo.path(), "fine");
    std::fs::write(
        repo.path().join(".balls/tasks/bl-ghost.json"),
        "{ not valid",
    )
    .unwrap();
    let out = bl(repo.path()).arg("list").output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("malformed"));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(stdout.contains(&ok_id));
}

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
    let json_start = s.find('{').unwrap();
    let v: serde_json::Value = serde_json::from_str(&s[json_start..]).unwrap();
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
}
