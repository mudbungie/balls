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
        .args(["update", &child, &format!("parent={parent}")])
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
    // Closed tasks are archived out of the tree; dep tree renders the
    // other five status markers.
    let repo = new_repo();
    init_in(repo.path());
    let ids = [
        create_task(repo.path(), "open-t"),
        create_task(repo.path(), "prog-t"),
        create_task(repo.path(), "review-t"),
        create_task(repo.path(), "block-t"),
        create_task(repo.path(), "deferred-t"),
    ];
    bl(repo.path())
        .args(["update", &ids[1], "status=in_progress"])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &ids[2], "status=review"])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &ids[3], "status=blocked"])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &ids[4], "status=deferred"])
        .assert()
        .success();
    let out = bl(repo.path()).args(["dep", "tree"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    // ASCII fallback engages when stdout is not a tty (the test
    // harness captures output). Each status flows through
    // `Display::status_glyph`'s ASCII branch.
    assert!(s.contains("[ ]"));
    assert!(s.contains("[>]"));
    assert!(s.contains("[?]"));
    assert!(s.contains("[!]"));
    assert!(s.contains("[-]"));
}

#[test]
fn dep_tree_json_emits_nested_structure() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "parent");
    let out = bl(repo.path())
        .args(["create", "child", "--parent", &parent])
        .output()
        .unwrap();
    let child = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let out = bl(repo.path())
        .args(["dep", "tree", "--json"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    let arr = v.as_array().unwrap();
    // Root is `parent`; its children array holds `child`.
    let root = arr.iter().find(|r| r["id"] == parent).unwrap();
    assert_eq!(root["children"][0]["id"], child);
    assert_eq!(root["status"], "open");
}

#[test]
fn dep_tree_unknown_id_errors() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["dep", "tree", "bl-ghost"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("bl-ghost"));
}

#[test]
fn bl_skill_dumps_skill_doc() {
    // `bl skill` emits the embedded SKILL.md verbatim and does not
    // require a balls-initialized repo.
    let repo = new_repo();
    let out = bl(repo.path()).arg("skill").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("balls"));
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
fn out_of_range_id_length_is_clamped_on_load() {
    let repo = new_repo();
    init_in(repo.path());
    let cfg_path = repo.path().join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["id_length"] = serde_json::json!(0);
    std::fs::write(&cfg_path, cfg.to_string()).unwrap();
    // bl create must succeed: the loader clamps id_length to its minimum,
    // so id generation cannot infinite-loop.
    let out = bl(repo.path())
        .args(["create", "after-clamp"])
        .output()
        .unwrap();
    assert!(out.status.success(), "bl create failed after id_length clamp");
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(id.len(), "bl-".len() + 4);
}
