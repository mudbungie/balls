//! Close/drop stories: 33–39, 62.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn story_33_review_then_close() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);
    std::fs::write(wt.join("feature.txt"), "code").unwrap();

    // Agent submits for review (from worktree — safe, worktree stays)
    bl(&wt)
        .args(["review", &id, "-m", "implemented"])
        .assert()
        .success();
    assert!(repo.path().join("feature.txt").exists());
    assert!(wt.exists());

    // Reviewer closes from repo root
    bl(repo.path())
        .args(["close", &id])
        .assert()
        .success();
    assert!(!wt.exists());
    assert!(!claims_dir(repo.path()).join(&id).exists());
    assert!(!discover_tasks_dir(repo.path()).join(format!("{id}.json")).exists());
}

#[test]
fn story_34_close_with_message_is_in_git_history() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["close", &id, "-m", "all done"])
        .assert()
        .success();
    // Archived: task file gone, main log clean, state branch has the close.
    assert!(!discover_tasks_dir(repo.path()).join(format!("{id}.json")).exists());
    let main_log = git(repo.path(), &["log", "--oneline", "main"]);
    assert!(
        !main_log.contains(&format!("close {id}")),
        "main must not carry balls close commits: {main_log}"
    );
    let state_log = git_state(repo.path(), &["log", "--oneline", "balls/tasks"]);
    assert!(
        state_log.contains(&format!("close {id}")),
        "state branch should record the close: {state_log}"
    );
}

#[test]
fn story_35_closing_dep_unblocks_dependent() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "a");
    let b = create_task_full(repo.path(), "b", 3, &[&a], &[]);

    bl_as(repo.path(), "alice")
        .args(["claim", &a])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["close", &a, "-m", "done"])
        .assert()
        .success();

    let out = bl(repo.path()).arg("ready").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains(&b));
}

#[test]
fn story_36_parent_completion_reaches_100() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "parent");
    let c1 = create_task(repo.path(), "c1");
    let c2 = create_task(repo.path(), "c2");
    bl(repo.path())
        .args(["update", &c1, &format!("parent={parent}")])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &c2, &format!("parent={parent}")])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &c1, "status=closed"])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &c2, "status=closed"])
        .assert()
        .success();

    let out = bl(repo.path())
        .args(["show", &parent, "--json"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["completion"], 1.0);
}

#[test]
fn story_38_drop_resets_task() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["drop", &id])
        .assert()
        .success();

    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "open");
    assert!(j["claimed_by"].is_null());
    assert!(j["branch"].is_null());
    assert!(!worktree_path(repo.path(), &id).exists());
    assert!(!claims_dir(repo.path()).join(&id).exists());
}

#[test]
fn story_39_drop_uncommitted_requires_force() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let wt = worktree_path(repo.path(), &id);
    std::fs::write(wt.join("dirty.txt"), "work").unwrap();
    bl(repo.path())
        .args(["drop", &id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("uncommitted"));
    bl(repo.path())
        .args(["drop", &id, "--force"])
        .assert()
        .success();
}

#[test]
fn story_62_resume_claimed_task_after_session_restart() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "ongoing");
    bl_as(repo.path(), "agent-alpha")
        .args(["claim", &id])
        .assert()
        .success();

    let out = bl_as(repo.path(), "agent-alpha")
        .args(["prime", "--json"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let json_start = s.find('{').unwrap();
    let v: serde_json::Value = serde_json::from_str(&s[json_start..]).unwrap();
    assert_eq!(v["identity"], "agent-alpha");
    assert_eq!(v["claimed"].as_array().unwrap().len(), 1);
}

#[test]
fn close_child_updates_parent_closed_children() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "parent");
    let child = create_task(repo.path(), "child");
    bl(repo.path())
        .args(["update", &child, &format!("parent={parent}")])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["claim", &child])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["close", &child])
        .assert()
        .success();

    // Child task file archived
    assert!(!discover_tasks_dir(repo.path()).join(format!("{child}.json")).exists());
    // Parent records the archived child
    let j = read_task_json(repo.path(), &parent);
    let cc = j["closed_children"].as_array().unwrap();
    assert_eq!(cc.len(), 1);
    assert_eq!(cc[0]["id"], child);

    // Show parent displays archived children
    let out = bl(repo.path())
        .args(["show", &parent])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains("[archived]"));
    assert!(s.contains("100%"));
}

#[test]
fn close_child_succeeds_when_parent_already_archived() {
    // Regression: close_and_archive used to unconditionally stage the
    // parent's JSON+notes, which aborted with "pathspec did not match"
    // when the parent had already been archived from the state branch.
    //
    // Claim is sequenced before the child's parent link so the bl-c79c
    // "no claim on a parent with live children" guard doesn't fire —
    // we're regression-testing the close path, not the claim guard.
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "parent");
    let child = create_task(repo.path(), "child");

    // Archive the parent before the child points at it.
    bl_as(repo.path(), "alice")
        .args(["claim", &parent])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &child, &format!("parent={parent}")])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["close", &parent])
        .assert()
        .success();
    assert!(!discover_tasks_dir(repo.path()).join(format!("{parent}.json")).exists());

    // Closing the child must still succeed even though its `parent`
    // field points at an archived task whose file is gone.
    bl_as(repo.path(), "alice")
        .args(["claim", &child])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["close", &child])
        .assert()
        .success();
    assert!(!discover_tasks_dir(repo.path()).join(format!("{child}.json")).exists());
}

#[test]
fn close_prints_cd_path() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "t");
    bl_as(repo.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    let out = bl(repo.path())
        .args(["close", &id])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    // Close outputs the repo root path (no "cd" prefix — machine-readable)
    assert!(s.contains(&repo.path().to_string_lossy().to_string()));
}

// review-vs-main merge tests moved to `close_drop_review.rs`.
