//! Coverage tests for write commands: update, dep, init edge cases.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn update_parse_error_without_equals() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl(repo.path())
        .args(["update", &id, "just_a_field_no_equals"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected field=value"));
}

#[test]
fn update_parent_to_null() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "parent");
    let child = create_task(repo.path(), "child");
    bl(repo.path())
        .args(["update", &child, &format!("parent={}", parent)])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &child);
    assert_eq!(j["parent"], parent);
    bl(repo.path())
        .args(["update", &child, "parent=null"])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &child);
    assert!(j["parent"].is_null());
    bl(repo.path())
        .args(["update", &child, "parent="])
        .assert()
        .success();
    let j = read_task_json(repo.path(), &child);
    assert!(j["parent"].is_null());
}

#[test]
fn update_bad_priority_value() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl(repo.path())
        .args(["update", &id, "priority=notanumber"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("priority"));
}

#[test]
fn update_bad_status_value() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl(repo.path())
        .args(["update", &id, "status=bogus"])
        .assert()
        .failure();
}

#[test]
fn dep_rm_nonexistent_dep_noop() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl(repo.path())
        .args(["dep", "rm", &id, "bl-xxxx"])
        .assert()
        .success();
}

#[test]
fn dep_add_already_present_noop() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task_full(repo.path(), "b", 3, &[&a], &[]);
    bl(repo.path())
        .args(["dep", "add", &b, &a])
        .assert()
        .success();
}

#[test]
fn dep_tree_status_markers() {
    // Closed tasks are archived (deleted), so dep tree shows 4 statuses.
    let repo = new_repo();
    init_in(repo.path());
    let ids = [
        create_task(repo.path(), "open-t"),
        create_task(repo.path(), "prog-t"),
        create_task(repo.path(), "block-t"),
        create_task(repo.path(), "deferred-t"),
    ];
    bl(repo.path())
        .args(["update", &ids[1], "status=in_progress"])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &ids[2], "status=blocked"])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &ids[3], "status=deferred"])
        .assert()
        .success();
    let out = bl(repo.path()).args(["dep", "tree"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("[ ]"));
    assert!(s.contains("[~]"));
    assert!(s.contains("[!]"));
    assert!(s.contains("[-]"));
}

#[test]
fn init_with_existing_gitignore_no_trailing_newline() {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@t"]);
    git(dir.path(), &["config", "user.name", "t"]);
    std::fs::write(dir.path().join(".gitignore"), "target/").unwrap();
    bl(dir.path()).arg("init").assert().success();
    let gi = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gi.contains("target/"));
    assert!(gi.contains(".balls/local"));
}

#[test]
fn init_sets_git_user_when_unset() {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    let home = tmp();
    bl(dir.path())
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", home.path().join("gitconfig"))
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .arg("init")
        .assert()
        .success();
    let email = git(dir.path(), &["config", "user.email"]);
    assert!(email.contains("balls"));
}

#[test]
fn init_with_partially_existing_gitignore() {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@t"]);
    git(dir.path(), &["config", "user.name", "t"]);
    std::fs::write(dir.path().join(".gitignore"), ".balls/local\n").unwrap();
    bl(dir.path()).arg("init").assert().success();
    let gi = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert_eq!(
        gi.lines().filter(|l| l.trim() == ".balls/local").count(),
        1
    );
}

#[test]
fn id_collision_retry_triggered() {
    let repo = new_repo();
    init_in(repo.path());
    let cfg_path = repo.path().join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["id_length"] = serde_json::json!(1);
    std::fs::write(&cfg_path, cfg.to_string()).unwrap();
    let mut ids = std::collections::HashSet::new();
    for i in 0..20 {
        let out = bl(repo.path())
            .args(["create", &format!("collision-{}", i)])
            .output()
            .unwrap();
        if !out.status.success() {
            break;
        }
        let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert!(ids.insert(id));
    }
    assert!(ids.len() >= 5);
}
