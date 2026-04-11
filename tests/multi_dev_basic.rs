//! Multi-dev basic syncing (non-conflict): stories 45, 46, 48, 53, 54, 55, 56.

mod common;

use common::*;

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
fn story_45_sync_no_remote_changes() {
    let (_r, alice, _bob) = three_way();
    create_task(alice.path(), "a");
    bl(alice.path()).arg("sync").assert().success();
}

#[test]
fn story_46_sync_nonconflicting_remote_changes() {
    let (_r, alice, bob) = three_way();
    let a_id = create_task(alice.path(), "alice task");
    bl(alice.path()).arg("sync").assert().success();

    let b_id = create_task(bob.path(), "bob task");
    bl(bob.path()).arg("sync").assert().success();

    assert!(bob
        .path()
        .join(".balls/tasks")
        .join(format!("{}.json", a_id))
        .exists());

    bl(alice.path()).arg("sync").assert().success();
    assert!(alice
        .path()
        .join(".balls/tasks")
        .join(format!("{}.json", b_id))
        .exists());
}

#[test]
fn story_48_sync_offline_graceful() {
    let (_r, alice, _bob) = three_way();
    git(
        alice.path(),
        &["remote", "set-url", "origin", "/tmp/nope-does-not-exist.git"],
    );
    create_task(alice.path(), "offline");
    let _ = bl(alice.path()).arg("sync").output();
    bl(alice.path()).arg("list").assert().success();
}

#[test]
fn story_53_different_tasks_no_conflict() {
    let (_r, alice, bob) = three_way();
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    create_task(alice.path(), "alice only");
    create_task(bob.path(), "bob only");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();
    bl(alice.path()).arg("sync").assert().success();

    let out_a = bl(alice.path()).arg("list").output().unwrap();
    let out_b = bl(bob.path()).arg("list").output().unwrap();
    let sa = String::from_utf8_lossy(&out_a.stdout).to_string();
    let sb = String::from_utf8_lossy(&out_b.stdout).to_string();
    assert!(sa.contains("alice only"));
    assert!(sa.contains("bob only"));
    assert!(sb.contains("alice only"));
    assert!(sb.contains("bob only"));
}

#[test]
fn story_54_dev_b_sees_dev_a_tasks_after_clone() {
    let remote = new_bare_remote();
    let alice = clone_from_remote(remote.path(), "alice");
    bl(alice.path()).arg("init").assert().success();
    let id_a = create_task(alice.path(), "from alice");
    let id_b = create_task(alice.path(), "also from alice");
    push(alice.path());

    let bob = clone_from_remote(remote.path(), "bob");
    bl(bob.path()).arg("init").assert().success();
    let out = bl(bob.path()).arg("list").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(s.contains(&id_a));
    assert!(s.contains(&id_b));
}

#[test]
fn story_55_claimed_by_a_hidden_from_b_ready() {
    let (_r, alice, bob) = three_way();
    let id = create_task(alice.path(), "shared");
    bl(alice.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id])
        .assert()
        .success();
    bl(alice.path()).arg("sync").assert().success();

    bl(bob.path()).arg("sync").assert().success();
    let out = bl(bob.path()).arg("ready").output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(!s.contains(&id));
}

#[test]
fn story_56_many_agents_claim_distinct_tasks() {
    let (_r, alice, bob) = three_way();
    let id1 = create_task(alice.path(), "t1");
    let id2 = create_task(alice.path(), "t2");
    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();

    bl_as(alice.path(), "alice")
        .args(["claim", &id1])
        .assert()
        .success();
    bl_as(bob.path(), "bob")
        .args(["claim", &id2])
        .assert()
        .success();

    bl(alice.path()).arg("sync").assert().success();
    bl(bob.path()).arg("sync").assert().success();
    bl(alice.path()).arg("sync").assert().success();

    let j1 = read_task_json(alice.path(), &id1);
    let j2 = read_task_json(alice.path(), &id2);
    assert_eq!(j1["status"], "in_progress");
    assert_eq!(j2["status"], "in_progress");
    assert_eq!(j1["claimed_by"], "alice");
    assert_eq!(j2["claimed_by"], "bob");
}
