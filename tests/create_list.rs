//! Create/list/show stories: 4–16, 76, 78, 79.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn story_4_create_with_title_only() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "hello");
    assert!(id.starts_with("bl-"));
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["title"], "hello");
    assert_eq!(j["status"], "open");
    assert_eq!(j["type"], "task");
    assert_eq!(j["priority"], 3);
}

#[test]
fn story_5_create_with_all_options() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "parent");
    let dep = create_task(repo.path(), "dep");
    let id = create_task_full(repo.path(), "child", 1, &[&dep], &["auth", "api"]);
    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["priority"], 1);
    assert_eq!(j["depends_on"][0], dep);
    assert_eq!(j["tags"][0], "auth");
    assert_eq!(j["tags"][1], "api");
    bl(repo.path())
        .args(["update", &id, &format!("parent={parent}")])
        .assert()
        .success();
}

#[test]
fn story_6_create_with_nonexistent_dep() {
    let repo = new_repo();
    init_in(repo.path());
    bl(repo.path())
        .args(["create", "x", "--dep", "bl-0000"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn story_7_create_as_child_of_parent() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "parent");
    bl(repo.path())
        .args(["create", "child", "--parent", &parent])
        .assert()
        .success();
    let pj = read_task_json(repo.path(), &parent);
    assert_eq!(pj["title"], "parent");
}

#[test]
fn story_8_circular_dep_at_create_time() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task_full(repo.path(), "b", 3, &[&a], &[]);
    bl(repo.path())
        .args(["dep", "add", &a, &b])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cycle"));
}

#[test]
fn story_10_list_open_tasks() {
    let repo = new_repo();
    init_in(repo.path());
    create_task_full(repo.path(), "low", 4, &[], &[]);
    create_task_full(repo.path(), "high", 1, &[], &[]);
    let out = bl(repo.path()).arg("list").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let low_pos = s.find("low").unwrap();
    let high_pos = s.find("high").unwrap();
    assert!(high_pos < low_pos);
}

#[test]
fn story_11_list_filtered_by_status() {
    // Titles deliberately use non-hex characters: 4-hex bl-XXXX IDs in
    // the listing can otherwise collide with short titles like "aa"/"bb".
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "blocked-one");
    bl(repo.path())
        .args(["update", &a, "status=blocked"])
        .assert()
        .success();
    create_task(repo.path(), "open-one");
    let out = bl(repo.path())
        .args(["list", "--status", "blocked"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("blocked-one"));
    assert!(!s.contains("open-one"));
}

#[test]
fn story_12_list_filtered_by_priority() {
    let repo = new_repo();
    init_in(repo.path());
    create_task_full(repo.path(), "p1", 1, &[], &[]);
    create_task_full(repo.path(), "p2", 2, &[], &[]);
    let out = bl(repo.path())
        .args(["list", "-p", "1"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("p1"));
    assert!(!s.contains("p2"));
}

#[test]
fn story_13_list_by_tag() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task_full(repo.path(), "alpha-task", 3, &[], &["auth"]);
    let b = create_task_full(repo.path(), "beta-task", 3, &[], &["ui"]);
    let out = bl(repo.path())
        .args(["list", "--tag", "auth"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains(&a));
    assert!(!s.contains(&b));
}

#[test]
fn story_14_list_children_of_parent() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "parent");
    let out1 = bl(repo.path())
        .args(["create", "kid-one", "--parent", &parent])
        .output()
        .unwrap();
    let kid1 = String::from_utf8_lossy(&out1.stdout).trim().to_string();
    let out2 = bl(repo.path())
        .args(["create", "kid-two", "--parent", &parent])
        .output()
        .unwrap();
    let kid2 = String::from_utf8_lossy(&out2.stdout).trim().to_string();
    let lonely = create_task(repo.path(), "lonely-task");
    let out = bl(repo.path())
        .args(["list", "--parent", &parent])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains(&kid1));
    assert!(s.contains(&kid2));
    assert!(!s.contains(&lonely));
}

#[test]
fn story_15_show_with_full_detail() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task_full(repo.path(), "detailed", 2, &[], &["x"]);
    let out = bl(repo.path()).args(["show", &id]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains(&id));
    assert!(s.contains("detailed"));
    // Priority renders as a glyph in the header; tag confirms task data
    // wires through end-to-end without scraping the visual indicator.
    assert!(s.contains("tags: x"));
}

#[test]
fn story_15_show_json() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "jsontask");
    let out = bl(repo.path())
        .args(["show", &id, "--json"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["task"]["title"], "jsontask");
    assert_eq!(v["dep_blocked"], false);
    // Non-epic tasks do not carry a `progress` object.
    assert!(v.get("progress").is_none());
}

#[test]
fn show_json_on_epic_emits_progress_object() {
    let repo = new_repo();
    init_in(repo.path());
    let epic = bl(repo.path())
        .args(["create", "myepic", "-t", "epic"])
        .output()
        .unwrap();
    let epic = String::from_utf8_lossy(&epic.stdout).trim().to_string();
    bl(repo.path())
        .args(["create", "kid", "--parent", &epic])
        .assert()
        .success();
    let out = bl(repo.path())
        .args(["show", &epic, "--json"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["progress"]["closed"], 0);
    assert_eq!(v["progress"]["total"], 1);
}

#[test]
fn story_16_list_all_includes_deferred() {
    // Closed tasks are archived (not on disk). --all shows deferred tasks
    // that `list` would normally hide.
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "deferred-thing");
    bl(repo.path())
        .args(["update", &a, "status=deferred"])
        .assert()
        .success();
    let out = bl(repo.path()).arg("list").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    // Default list excludes closed but shows deferred
    assert!(s.contains(&a));
    // --all also shows it
    let out = bl(repo.path()).args(["list", "--all"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains(&a));
}

#[test]
fn story_76_malformed_task_json() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "good");
    std::fs::write(
        repo.path().join(".balls/tasks").join(format!("{id}.json")),
        "not valid json",
    )
    .unwrap();
    let out = bl(repo.path()).arg("repair").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("BAD:"));
}

#[test]
fn story_78_many_tasks_perf() {
    let repo = new_repo();
    init_in(repo.path());
    for i in 0..50 {
        create_task(repo.path(), &format!("task {i}"));
    }
    let start = std::time::Instant::now();
    bl(repo.path()).arg("list").assert().success();
    let elapsed = start.elapsed();
    assert!(elapsed.as_secs() < 5);
}

#[test]
fn story_79_id_collision_retry() {
    let repo = new_repo();
    init_in(repo.path());
    let mut ids = std::collections::HashSet::new();
    for _ in 0..10 {
        let id = create_task(repo.path(), "same title");
        assert!(ids.insert(id));
    }
}
