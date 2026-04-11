//! Multi-dev conflict stories: 47, 50, 51, 52, 58, plus concurrency and the
//! manual resolve command.

mod common;

use common::*;
use std::thread;

fn three_way() -> (Repo, Repo, Repo) {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    bl(alice.path()).arg("init").assert().success();
    push(alice.path());

    let bob = clone_from_remote(remote.path(), "bob");
    bl(bob.path()).arg("init").assert().success();
    (remote, alice, bob)
}

#[test]
fn story_47_sync_conflicting_tasks_auto_resolve() {
    let (_r, alice, bob) = three_way();

    let id = create_task(alice.path(), "shared");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl(alice.path())
        .args(["update", &id, "priority=1"])
        .assert()
        .success();
    bl(bob.path())
        .args(["update", &id, "priority=2", "--note", "bob note"])
        .assert()
        .success();

    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();
    bl(alice.path()).arg("sync").assert().success();

    let j_alice = read_task_json(alice.path(), &id);
    let j_bob = read_task_json(bob.path(), &id);
    assert_eq!(j_alice["priority"], j_bob["priority"]);
    let notes = j_bob["notes"].as_array().unwrap();
    assert!(notes.iter().any(|n| n["text"] == "bob note"));
}

#[test]
fn story_50_two_workers_claim_same_task() {
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "contested");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    bl_as(bob.path(), "bob")
        .args(["claim", &id])
        .assert()
        .success();

    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    let j = read_task_json(bob.path(), &id);
    assert_eq!(j["status"], "in_progress");
    let cb = j["claimed_by"].as_str().unwrap();
    assert!(cb == "alice" || cb == "bob");
}

#[test]
fn story_51_two_workers_close_same_task() {
    // Both close (archive) the same task. First push wins.
    // Second worker's sync sees the task already archived.
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "to close");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["update", &id, "status=closed", "--note", "alice closed"])
        .assert()
        .success();
    // Alice's close archived the task — file is gone
    assert!(!alice.path().join(format!(".balls/tasks/{}.json", id)).exists());

    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();
    // Bob now sees the task is gone (archived by alice)
    assert!(!bob.path().join(format!(".balls/tasks/{}.json", id)).exists());
}

#[test]
fn story_52_close_vs_update() {
    // Alice closes (archives), Bob updates priority. Sync handles the
    // delete-vs-modify conflict gracefully — system doesn't corrupt.
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "close vs update");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["update", &id, "status=closed", "--note", "closed by alice"])
        .assert()
        .success();
    bl_as(bob.path(), "bob")
        .args(["update", &id, "priority=1", "--note", "bob thought"])
        .assert()
        .success();

    // Both sides sync without crashing
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();
}

#[test]
fn story_58_offline_then_resolve() {
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "seed");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["update", &id, "priority=1", "--note", "alice"])
        .assert()
        .success();
    bl(alice.path()).arg("sync").assert().success();

    bl_as(bob.path(), "bob")
        .args(["update", &id, "priority=2", "--note", "bob offline"])
        .assert()
        .success();

    bl(bob.path()).arg("sync").assert().success();

    let j = read_task_json(bob.path(), &id);
    let notes = j["notes"].as_array().unwrap();
    assert!(notes.iter().any(|n| n["text"] == "alice"));
    assert!(notes.iter().any(|n| n["text"] == "bob offline"));
}

#[test]
fn concurrent_claims_local_flock() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "contested");

    let p1 = repo.path().to_path_buf();
    let p2 = repo.path().to_path_buf();
    let id1 = id.clone();
    let id2 = id.clone();

    let h1 = thread::spawn(move || {
        bl_as(&p1, "alice").args(["claim", &id1]).output().unwrap()
    });
    let h2 = thread::spawn(move || {
        bl_as(&p2, "bob").args(["claim", &id2]).output().unwrap()
    });
    let out1 = h1.join().unwrap();
    let out2 = h2.join().unwrap();

    let s1 = out1.status.success();
    let s2 = out2.status.success();
    assert!(s1 ^ s2, "exactly one claim must succeed, got {} {}", s1, s2);
}

#[test]
fn sync_push_retry_path_after_race() {
    // A race scenario: between bob's fetch and bob's push, alice pushes
    // again. Bob's first push will fail and trigger the fetch+merge+push
    // retry branch inside cmd_sync.
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "racy");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    // Both edit the same task independently.
    bl(alice.path())
        .args(["update", &id, "priority=1", "--note", "alice v1"])
        .assert()
        .success();
    bl(bob.path())
        .args(["update", &id, "priority=2", "--note", "bob v1"])
        .assert()
        .success();

    // Alice pushes first.
    bl(alice.path()).arg("sync").assert().success();

    // Before bob syncs, alice makes another update to keep the remote moving
    // underneath him — forces extra work inside bob's sync.
    bl(alice.path())
        .args(["update", &id, "--note", "alice v2"])
        .assert()
        .success();
    bl(alice.path()).arg("sync").assert().success();

    bl(bob.path()).arg("sync").assert().success();
    let j = read_task_json(bob.path(), &id);
    let notes = j["notes"].as_array().unwrap();
    assert!(notes.iter().any(|n| n["text"] == "alice v1"));
    assert!(notes.iter().any(|n| n["text"] == "bob v1"));
}

#[test]
fn sync_rejects_conflict_in_non_task_file() {
    // Sync should error cleanly when a conflict lands in a file outside
    // .balls/tasks/ — we don't know how to auto-resolve it.
    let (_r, alice, bob) = three_way();
    // Both repos edit a shared non-task file.
    std::fs::write(alice.path().join("shared.txt"), "alice side").unwrap();
    git(alice.path(), &["add", "shared.txt"]);
    git(alice.path(), &["commit", "-m", "alice shared"]);
    bl(alice.path()).arg("sync").assert().success();

    std::fs::write(bob.path().join("shared.txt"), "bob side").unwrap();
    git(bob.path(), &["add", "shared.txt"]);
    git(bob.path(), &["commit", "-m", "bob shared"]);

    // Bob's sync conflicts on a non-task file → sync should fail
    let out = bl(bob.path()).arg("sync").output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("unhandled conflict") || stderr.contains("conflict"));
}

#[test]
fn sync_resolve_command_on_single_file() {
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "r");

    let path = repo.path().join(".balls/tasks").join(format!("{}.json", id));
    let orig = std::fs::read_to_string(&path).unwrap();
    let ours = orig.replace("\"priority\": 3", "\"priority\": 1");
    let theirs = orig.replace("\"priority\": 3", "\"priority\": 2");
    let conflict = format!(
        "<<<<<<< HEAD\n{}=======\n{}>>>>>>> theirs\n",
        ours, theirs
    );
    std::fs::write(&path, conflict).unwrap();

    bl(repo.path())
        .args(["resolve", &format!(".balls/tasks/{}.json", id)])
        .assert()
        .success();

    let j = read_task_json(repo.path(), &id);
    let p = j["priority"].as_u64().unwrap();
    assert!(p == 1 || p == 2);
}
