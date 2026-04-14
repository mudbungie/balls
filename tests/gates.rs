//! Gates: post-review blockers that prevent a parent from closing
//! until linked child audit tasks are themselves closed.

mod common;

use common::*;
use predicates::prelude::*;

#[test]
fn link_add_accepts_gates_variant() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let child = create_task(repo.path(), "audit");
    bl(repo.path())
        .args(["link", "add", &parent, "gates", &child])
        .assert()
        .success()
        .stdout(predicate::str::contains("gates"));
    let j = read_task_json(repo.path(), &parent);
    let links = j["links"].as_array().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["link_type"], "gates");
    assert_eq!(links[0]["target"], child);
}

#[test]
fn close_rejects_when_gate_child_open() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let child = create_task(repo.path(), "audit");
    bl(repo.path())
        .args(["link", "add", &parent, "gates", &child])
        .assert()
        .success();

    // Try to close the parent via `bl update status=closed` (unclaimed
    // path). This must fail with a message that names the blocker.
    bl(repo.path())
        .args(["update", &parent, "status=closed"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("blocked by open gate"))
        .stderr(predicate::str::contains(&child));

    // Parent file is still present; the close was rejected before any
    // state change. Status is whatever it was (open, unaffected).
    let j = read_task_json(repo.path(), &parent);
    assert_eq!(j["status"], "open");
}

#[test]
fn close_succeeds_after_gate_child_closes() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let child = create_task(repo.path(), "audit");
    bl(repo.path())
        .args(["link", "add", &parent, "gates", &child])
        .assert()
        .success();
    // Close the gate child first.
    bl(repo.path())
        .args(["update", &child, "status=closed"])
        .assert()
        .success();
    // Now the parent close is allowed.
    bl(repo.path())
        .args(["update", &parent, "status=closed"])
        .assert()
        .success();
    assert!(!repo
        .path()
        .join(".balls/tasks")
        .join(format!("{parent}.json"))
        .exists());
}

#[test]
fn close_worktree_path_also_enforces_gates() {
    // The claimed-task close path goes through review::close_worktree,
    // not the update path. This covers the second enforcement site.
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let child = create_task(repo.path(), "audit");
    bl(repo.path())
        .args(["link", "add", &parent, "gates", &child])
        .assert()
        .success();

    bl_as(repo.path(), "alice")
        .args(["claim", &parent])
        .assert()
        .success();
    bl_as(repo.path(), "alice")
        .args(["close", &parent])
        .assert()
        .failure()
        .stderr(predicate::str::contains("blocked by open gate"));

    // Worktree still exists — close was rejected before teardown.
    assert!(repo.path().join(".balls-worktrees").join(&parent).exists());
}

#[test]
fn gate_child_closes_normally_without_affecting_parent() {
    // Gate semantics only block the parent. Closing the child itself
    // is an ordinary close and must not be blocked by the back-link.
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let child = create_task(repo.path(), "audit");
    bl(repo.path())
        .args(["link", "add", &parent, "gates", &child])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &child, "status=closed"])
        .assert()
        .success();
}

#[test]
fn multiple_gates_all_named_in_error() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let g1 = create_task(repo.path(), "sec");
    let g2 = create_task(repo.path(), "doc");
    let g3 = create_task(repo.path(), "cov");
    for g in [&g1, &g2, &g3] {
        bl(repo.path())
            .args(["link", "add", &parent, "gates", g])
            .assert()
            .success();
    }
    let out = bl(repo.path())
        .args(["update", &parent, "status=closed"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(&g1));
    assert!(stderr.contains(&g2));
    assert!(stderr.contains(&g3));
    assert!(stderr.contains("gates"));
}

#[test]
fn link_rm_drops_gate_and_unblocks_close() {
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let child = create_task(repo.path(), "audit");
    bl(repo.path())
        .args(["link", "add", &parent, "gates", &child])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &parent, "status=closed"])
        .assert()
        .failure();
    // Explicitly drop the gate — leaves a commit trail but lets the
    // parent close.
    bl(repo.path())
        .args(["link", "rm", &parent, "gates", &child])
        .assert()
        .success();
    bl(repo.path())
        .args(["update", &parent, "status=closed"])
        .assert()
        .success();
}

#[test]
fn dep_tree_handles_mixed_dep_and_gates() {
    // A task holding both `depends_on` and `gates` links should
    // render through `bl dep tree` without error and show both
    // relationships. Guards against a future regression where the
    // tree walker trips on unrecognised link types.
    let repo = new_repo();
    init_in(repo.path());
    let base = create_task(repo.path(), "base");
    let parent = create_task_full(repo.path(), "parent", 3, &[&base], &[]);
    let gate = create_task(repo.path(), "gate");
    bl(repo.path())
        .args(["link", "add", &parent, "gates", &gate])
        .assert()
        .success();
    let out = bl(repo.path())
        .args(["dep", "tree"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains(&parent));
    assert!(s.contains(&base));
}

#[test]
fn gate_children_are_claim_ready() {
    // Locks in the documented semantics: `gates` blocks close of
    // the parent, NOT claim of the child. A gate target should
    // appear in `bl ready` the moment it exists, regardless of
    // parent state. (Contrast with `dep`, which suppresses the
    // child from ready until the dep closes.)
    let repo = new_repo();
    init_in(repo.path());
    let parent = create_task(repo.path(), "impl");
    let gate = create_task(repo.path(), "audit");
    bl(repo.path())
        .args(["link", "add", &parent, "gates", &gate])
        .assert()
        .success();

    let out = bl(repo.path())
        .args(["ready"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains(&gate),
        "gate child must be claim-ready: {s}"
    );
    assert!(
        s.contains(&parent),
        "parent with only gate links must also be claim-ready: {s}"
    );
}

