//! bl-2057 — `bl remaster`: the recovery surface. Join a standalone
//! repo to a shared hub (reconcile), re-target, go back standalone
//! (`--detach`), and survive an id clash on import.

mod common;

use common::*;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

fn seed_config(repo: &Path, state_remote: &str) {
    let balls = repo.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    fs::write(
        balls.join("config.json"),
        format!(
            r#"{{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees","state_remote":"{state_remote}"}}"#
        ),
    )
    .unwrap();
}

fn add_remote(repo: &Path, name: &str, target: &Path) {
    git(repo, &["remote", "add", name, &target.to_string_lossy()]);
}

fn state_sha(bare: &Path) -> String {
    git(bare, &["rev-parse", "refs/heads/balls/tasks"])
        .trim()
        .to_string()
}

/// A hub bare repo carrying `balls/tasks` with one task; returns
/// (code remote, hub remote, the hub task's id).
fn hub_with_task() -> (Repo, Repo, String) {
    let code = new_bare_remote();
    let hub = new_bare_remote();
    let founder = clone_from_remote(code.path(), "founder");
    add_remote(founder.path(), "hub", hub.path());
    seed_config(founder.path(), "hub");
    bl(founder.path()).arg("init").assert().success();
    git(founder.path(), &["push", "origin", "main"]);
    let hid = create_task(founder.path(), "Hub Owned");
    bl(founder.path()).arg("sync").assert().success();
    (code, hub, hid)
}

/// A standalone repo (its own code remote, no hub link) with one
/// local-only task.
fn standalone_with_task(name: &str) -> (Repo, Repo, String) {
    let code = new_bare_remote();
    let repo = clone_from_remote(code.path(), name);
    bl(repo.path()).arg("init").assert().success();
    let id = create_task(repo.path(), "Local Only");
    (code, repo, id)
}

#[test]
fn remaster_joins_standalone_repo_to_hub_and_can_push_back() {
    let (_code, hub, hid) = hub_with_task();
    let (_cc, carol, cid) = standalone_with_task("carol");

    add_remote(carol.path(), "hub", hub.path());
    bl(carol.path())
        .args(["remaster", "hub"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 task(s) replayed, 0 renamed"))
        .stdout(predicate::str::contains("per-clone state_remote=`hub`"));

    let tasks = carol.path().join(".balls/tasks");
    assert!(tasks.join(format!("{hid}.json")).exists(), "hub task adopted");
    assert!(tasks.join(format!("{cid}.json")).exists(), "local task kept");
    let local_cfg =
        fs::read_to_string(carol.path().join(".balls/local/config.json")).unwrap();
    assert!(local_cfg.contains("\"state_remote\""));
    assert!(local_cfg.contains("hub"));

    // The joined branch descends from the hub, so a normal sync
    // pushes the local-only task up.
    bl(carol.path()).arg("sync").assert().success();
    let on_hub = git(
        hub.path(),
        &["ls-tree", "--name-only", "refs/heads/balls/tasks", ".balls/tasks/"],
    );
    assert!(
        on_hub.contains(&format!("{cid}.json")),
        "joined repo can push its task to the hub: {on_hub}"
    );
}

#[test]
fn remaster_is_idempotent() {
    let (_code, hub, _hid) = hub_with_task();
    let (_cc, carol, _cid) = standalone_with_task("carol");
    add_remote(carol.path(), "hub", hub.path());
    bl(carol.path()).args(["remaster", "hub"]).assert().success();
    bl(carol.path())
        .args(["remaster", "hub"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already up to date with `hub`"));
}

#[test]
fn remaster_commit_writes_the_committed_pointer() {
    // bl-82a4: state_remote moved from .balls/config.json into the
    // dedicated federation pointer at .balls/master.json.
    let (_code, hub, _hid) = hub_with_task();
    let (_cc, carol, _cid) = standalone_with_task("carol");
    add_remote(carol.path(), "hub", hub.path());
    bl(carol.path())
        .args(["remaster", "hub", "--commit"])
        .assert()
        .success()
        .stdout(predicate::str::contains("committed .balls/master.json"));
    let pointer = fs::read_to_string(carol.path().join(".balls/master.json")).unwrap();
    assert!(pointer.contains("\"state_remote\""));
    assert!(pointer.contains("hub"));
}

#[test]
fn remaster_detach_goes_standalone() {
    let (_code, hub, hid) = hub_with_task();
    let (_cc, carol, cid) = standalone_with_task("carol");
    add_remote(carol.path(), "hub", hub.path());
    bl(carol.path()).args(["remaster", "hub"]).assert().success();
    let hub_sha = state_sha(hub.path());

    bl(carol.path())
        .args(["remaster", "--detach"])
        .assert()
        .success()
        .stdout(predicate::str::contains("detached"));

    // Tasks survive the re-root.
    let tasks = carol.path().join(".balls/tasks");
    assert!(tasks.join(format!("{hid}.json")).exists());
    assert!(tasks.join(format!("{cid}.json")).exists());
    // History no longer descends from the hub.
    let wt = carol.path().join(".balls/worktree");
    assert!(
        !git_ok(
            &wt,
            &["merge-base", "--is-ancestor", &hub_sha, "balls/tasks"]
        ),
        "detach must sever shared ancestry with the hub"
    );
    // The link is cleared to origin (standalone).
    let lc = fs::read_to_string(carol.path().join(".balls/local/config.json")).unwrap();
    assert!(lc.contains("origin"));
}

#[test]
fn remaster_renames_id_clash_on_import() {
    let (_code, hub, hid) = hub_with_task();
    let (_cc, carol, _cid) = standalone_with_task("carol");

    // Hand-write a DIFFERENT task that collides on the hub task's id,
    // and commit it on carol's state branch (SPEC §11 hand-edit).
    let clash = format!(
        r#"{{"id":"{hid}","title":"Carol Clash","type":"task","priority":3,"status":"open","parent":null,"created_at":"2020-01-01T00:00:00Z","updated_at":"2020-01-01T00:00:00Z","closed_at":null,"claimed_by":null,"branch":null}}"#
    );
    fs::write(
        carol.path().join(".balls/tasks").join(format!("{hid}.json")),
        clash,
    )
    .unwrap();
    let wt = carol.path().join(".balls/worktree");
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "hand: clash", "--no-verify"]);

    add_remote(carol.path(), "hub", hub.path());
    bl(carol.path())
        .args(["remaster", "hub"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 renamed"));

    // The hub's task keeps the id; carol's clashing task is re-imported
    // under a fresh id. Both survive; neither is merged into the other.
    let tasks = carol.path().join(".balls/tasks");
    let hub_task = fs::read_to_string(tasks.join(format!("{hid}.json"))).unwrap();
    assert!(hub_task.contains("Hub Owned"), "hub task keeps its id");
    let mut carol_clash_found = false;
    for e in fs::read_dir(&tasks).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|s| s.to_str()) == Some("json")
            && p.file_stem().and_then(|s| s.to_str()) != Some(&hid)
            && fs::read_to_string(&p).unwrap().contains("Carol Clash")
        {
            carol_clash_found = true;
        }
    }
    assert!(carol_clash_found, "clashing task re-imported under a new id");
}

#[test]
fn remaster_needs_target_or_detach() {
    let (_code, repo, _id) = standalone_with_task("solo");
    bl(repo.path())
        .arg("remaster")
        .assert()
        .failure()
        .stderr(predicate::str::contains("needs a TARGET"));
}

#[test]
fn remaster_detach_rejects_a_target() {
    let (_code, repo, _id) = standalone_with_task("solo");
    bl(repo.path())
        .args(["remaster", "hub", "--detach"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("takes no TARGET"));
}

#[test]
fn remaster_unknown_remote_errors() {
    let (_code, repo, _id) = standalone_with_task("solo");
    bl(repo.path())
        .args(["remaster", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no git remote `nope`"));
}

#[test]
fn remaster_target_without_state_branch_errors() {
    let (_code, repo, _id) = standalone_with_task("solo");
    let empty = new_bare_remote();
    add_remote(repo.path(), "hub", empty.path());
    bl(repo.path())
        .args(["remaster", "hub"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("has no balls/tasks"));
}

#[test]
fn remaster_fetch_failure_errors() {
    let (_code, repo, _id) = standalone_with_task("solo");
    git(
        repo.path(),
        &["remote", "add", "hub", "/no/such/path/hub.git"],
    );
    bl(repo.path())
        .args(["remaster", "hub"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("could not fetch from `hub`"));
}

#[test]
fn remaster_rejects_stealth_repo() {
    let repo = new_repo();
    bl(repo.path()).args(["init", "--stealth"]).assert().success();
    bl(repo.path())
        .args(["remaster", "hub"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("non-stealth git-backed repo"));
}
