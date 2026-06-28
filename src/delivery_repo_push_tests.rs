//! `push_integration` tests (bl-2656): the fail-soft code-main push that
//! `close.post` runs after teardown — the symmetric twin of the tracker's
//! store push. Split from `delivery_repo_tests` for the 300-line cap; the
//! shared `project()` fixture is reused via `super::tests`.

use super::tests::project;
use super::*;
use crate::delivery::Repo;
use std::fs;

#[test]
fn push_integration_is_a_silent_no_op_without_an_origin() {
    // Stealth: no `origin` (git's to own) → the structural no-op, the twin of
    // the store push with no remote. No error, nothing pushed.
    let (_tmp, _root, p) = project();
    p.push_integration().unwrap();
}

#[test]
fn push_integration_pushes_the_integration_branch_to_origin() {
    let (tmp, root, p) = project();
    let origin = tmp.path().join("origin.git");
    Project::run(tmp.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]).unwrap();
    Project::run(&root, &["remote", "add", "origin", origin.to_str().unwrap()]).unwrap();

    p.push_integration().unwrap();

    // origin now carries `main` at local's tip — the delivery reached the remote.
    let local = Project::run(&root, &["rev-parse", "main"]).unwrap();
    let remote = Project::run(&origin, &["rev-parse", "main"]).unwrap();
    assert_eq!(local, remote);
}

#[test]
fn push_integration_is_fail_soft_when_origin_rejects_a_non_ff() {
    // The bl-c3c0 lagging-clone surface: origin moved under us. The delivery
    // already landed on local `main` (close.pre), so the rejected push must NOT
    // abort the close — it warns and leaves local ahead.
    let (tmp, root, p) = project();
    let origin = tmp.path().join("origin.git");
    Project::run(tmp.path(), &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]).unwrap();
    Project::run(&root, &["remote", "add", "origin", origin.to_str().unwrap()]).unwrap();
    Project::run(&root, &["push", "-q", "origin", "main"]).unwrap(); // origin == seed

    // A second clone advances origin beyond local — origin gains a commit local lacks.
    let other = tmp.path().join("other");
    Project::run(tmp.path(), &["clone", "-q", origin.to_str().unwrap(), other.to_str().unwrap()]).unwrap();
    Project::run(&other, &["config", "user.name", "other"]).unwrap();
    Project::run(&other, &["config", "user.email", "other@example.com"]).unwrap();
    fs::write(other.join("a.txt"), "a\n").unwrap();
    Project::run(&other, &["add", "-A"]).unwrap();
    Project::run(&other, &["commit", "-q", "-m", "other work"]).unwrap();
    Project::run(&other, &["push", "-q", "origin", "main"]).unwrap();

    // Local makes its OWN divergent commit (the just-delivered squash) → non-ff.
    fs::write(root.join("b.txt"), "b\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-q", "-m", "local delivery"]).unwrap();

    p.push_integration().unwrap(); // FAIL-SOFT: Ok despite the reject

    // origin did not take local's commit; local is left ahead for hand-recovery.
    assert!(!Project::ok(&origin, &["cat-file", "-e", "main:b.txt"]).unwrap());
    assert!(Project::ok(&root, &["cat-file", "-e", "main:b.txt"]).unwrap());
}
