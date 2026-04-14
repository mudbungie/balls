//! bl-e501: accept bare-hex task ids (without `bl-` prefix) on the CLI.
//!
//! Normalization happens at the CLI boundary, so every subcommand that takes
//! a task id must accept both `bl-534c` and `534c`.

mod common;

use common::*;

/// Strip the "bl-" prefix to get the bare-hex form of an id.
fn bare(id: &str) -> &str {
    id.strip_prefix("bl-").expect("id should start with bl-")
}

#[test]
fn show_accepts_bare_hex() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "example");

    bl(repo.path())
        .args(["show", bare(&id)])
        .assert()
        .success();
}

#[test]
fn claim_review_close_accept_bare_hex() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "worker-flow");
    let bare_id = bare(&id).to_string();

    bl_as(repo.path(), "agent")
        .args(["claim", &bare_id])
        .assert()
        .success();

    let wt = repo.path().join(".balls-worktrees").join(&id);
    std::fs::write(wt.join("work.txt"), "done").unwrap();

    bl_as(repo.path(), "agent")
        .args(["review", &bare_id, "-m", "ship"])
        .assert()
        .success();

    bl_as(repo.path(), "agent")
        .args(["close", &bare_id, "-m", "approved"])
        .assert()
        .success();
}

#[test]
fn drop_accepts_bare_hex() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "to-drop");
    let bare_id = bare(&id).to_string();

    bl_as(repo.path(), "agent")
        .args(["claim", &bare_id])
        .assert()
        .success();
    bl_as(repo.path(), "agent")
        .args(["drop", &bare_id])
        .assert()
        .success();

    let j = read_task_json(repo.path(), &id);
    assert_eq!(j["status"], "open");
}

#[test]
fn update_accepts_bare_hex() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "updatable");

    bl(repo.path())
        .args(["update", bare(&id), "--note", "a note"])
        .assert()
        .success();

    let notes = read_task_notes(repo.path(), &id);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["text"], "a note");
}

#[test]
fn dep_add_rm_tree_accept_bare_hex() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "task-a");
    let b = create_task(repo.path(), "task-b");

    bl(repo.path())
        .args(["dep", "add", bare(&a), bare(&b)])
        .assert()
        .success();

    bl(repo.path())
        .args(["dep", "tree", bare(&a)])
        .assert()
        .success();

    bl(repo.path())
        .args(["dep", "rm", bare(&a), bare(&b)])
        .assert()
        .success();
}

#[test]
fn link_add_rm_accept_bare_hex() {
    let repo = new_repo();
    init_in(repo.path());
    let a = create_task(repo.path(), "linkable-a");
    let b = create_task(repo.path(), "linkable-b");

    bl(repo.path())
        .args(["link", "add", bare(&a), "relates_to", bare(&b)])
        .assert()
        .success();

    bl(repo.path())
        .args(["link", "rm", bare(&a), "relates_to", bare(&b)])
        .assert()
        .success();
}

#[test]
fn create_parent_and_dep_accept_bare_hex() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "the parent");
    let dep = create_task(repo.path(), "the dep");

    let out = bl(repo.path())
        .args([
            "create",
            "child",
            "--parent",
            bare(&parent),
            "--dep",
            bare(&dep),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let child_id = String::from_utf8_lossy(&out.stdout).trim().to_string();

    let j = read_task_json(repo.path(), &child_id);
    assert_eq!(j["parent"], parent);
    assert_eq!(j["depends_on"][0], dep);
}

#[test]
fn list_parent_filter_accepts_bare_hex() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "top");

    let out = bl(repo.path())
        .args([
            "create",
            "under-top",
            "--parent",
            &parent,
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    let out = bl(repo.path())
        .args(["list", "--parent", bare(&parent)])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("under-top"));
}
