//! [`Project`] tests on throwaway project repos — every git act and its
//! idempotent re-run, and the direct squash.

use super::*;
use crate::delivery::Repo;
use std::fs;
use tempfile::TempDir;

/// A throwaway project repo on `main` with one seed commit. Returns the tempdir
/// (kept alive), its root, and a [`Project`]. Shared with the sibling
/// `gate_tests` module.
pub fn project() -> (TempDir, PathBuf, Project) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("proj");
    fs::create_dir(&root).unwrap();
    let g = |args: &[&str]| Project::run(&root, args).unwrap();
    g(&["init", "-q", "-b", "main"]);
    g(&["config", "user.name", "test"]);
    g(&["config", "user.email", "test@example.com"]);
    fs::write(root.join("seed.txt"), "seed\n").unwrap();
    g(&["add", "-A"]);
    g(&["commit", "-q", "-m", "seed"]);
    (tmp, root.clone(), Project::at(&root))
}

/// `main`'s tip subject — the delivery assertion surface.
pub fn tip(root: &Path) -> String {
    Project::run(root, &["log", "-1", "--format=%s", "main"]).unwrap().trim().to_string()
}

#[test]
fn materialize_creates_then_is_idempotent_then_reattaches_an_existing_branch() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");

    // Branch absent → create work/<id> off HEAD.
    p.materialize(&wt, "work/bl-x").unwrap();
    assert!(wt.join("seed.txt").exists());
    assert!(p.branch_exists("work/bl-x").unwrap());

    // Path present → no-op (no second `worktree add`, which would fail).
    p.materialize(&wt, "work/bl-x").unwrap();

    // Worktree gone but branch kept → reattach the existing branch.
    p.release(&wt).unwrap();
    assert!(!wt.exists() && p.branch_exists("work/bl-x").unwrap());
    p.materialize(&wt, "work/bl-x").unwrap();
    assert!(wt.join("seed.txt").exists());

    let _ = root;
}

#[test]
fn materialize_recovers_a_deleted_dir_with_a_stale_registration() {
    // The ordinary form of "absent" (bl-b404): the dir was rm -rf'd, not
    // `worktree remove`d, so git still holds a registration. A bare
    // `worktree add` aborts with "missing but already registered worktree";
    // materialize must prune the stale registration and re-materialize.
    let (tmp, _root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::remove_dir_all(&wt).unwrap(); // crash / tmp cleaner / human

    p.materialize(&wt, "work/bl-x").unwrap();
    assert!(wt.join("seed.txt").exists());
}

#[test]
fn release_removes_a_present_worktree_and_no_ops_when_absent() {
    let (tmp, _root, p) = project();
    let wt = tmp.path().join("wt");
    p.release(&wt).unwrap(); // absent → no-op
    p.materialize(&wt, "work/bl-x").unwrap();
    p.release(&wt).unwrap();
    assert!(!wt.exists());
}

#[test]
fn discard_removes_the_worktree_and_deletes_the_branch() {
    let (tmp, _root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    p.discard(&wt, "work/bl-x").unwrap();
    assert!(!wt.exists());
    assert!(!p.branch_exists("work/bl-x").unwrap());
    // Idempotent: both worktree and branch already gone.
    p.discard(&wt, "work/bl-x").unwrap();
}

#[test]
fn integration_is_the_project_head_branch() {
    let (_tmp, _root, p) = project();
    assert_eq!(p.integration().unwrap(), "main");
}

#[test]
fn is_git_repo_holds_for_a_worktree_and_a_bare_repo_but_not_a_plain_dir() {
    // The bl-4a88 precondition predicate, read by EXIT CODE: a normal work tree
    // is a repo, and so is a BARE one (the common balls deployment — delivery
    // works against it, so the gate must NOT reject it), while a plain dir is
    // not — the only case `claim`/`close` should abort on.
    let (_tmp, _root, p) = project();
    assert!(p.is_git_repo().unwrap());

    let bare = TempDir::new().unwrap();
    Project::run(bare.path(), &["init", "-q", "--bare", "-b", "main"]).unwrap();
    assert!(Project::at(bare.path()).is_git_repo().unwrap());

    let plain = TempDir::new().unwrap();
    assert!(!Project::at(plain.path()).is_git_repo().unwrap());
}

#[test]
fn deliver_captures_pending_work_and_squashes_it_onto_integration() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    // Uncommitted work in the code worktree — deliver must capture it.
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();

    assert_eq!(tip(&root), "Add feature [bl-x]");
    // The squash landed as ONE commit on main, parented on the seed.
    assert_eq!(Project::run(&root, &["show", "main:feature.txt"]).unwrap(), "shipped\n");
    assert_eq!(Project::run(&root, &["rev-list", "--count", "main"]).unwrap().trim(), "2");
}

#[test]
fn deliver_with_no_pending_work_but_a_committed_branch_still_squashes() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("f.txt"), "x\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-q", "-m", "wip"]).unwrap();

    // Nothing pending now (already committed) — capture is a clean no-op, the
    // squash still delivers the committed branch state.
    p.deliver(&wt, "work/bl-x", "main", "Land it [bl-x]", "[bl-x]").unwrap();
    assert_eq!(tip(&root), "Land it [bl-x]");
}

#[test]
fn deliver_is_a_no_op_for_an_empty_deliverable() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap(); // claimed, never worked → no diff

    p.deliver(&wt, "work/bl-x", "main", "nothing [bl-x]", "[bl-x]").unwrap();
    assert_eq!(tip(&root), "seed"); // integration untouched
}

#[test]
fn deliver_is_a_no_op_when_the_branch_was_never_made() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt"); // never materialized
    p.deliver(&wt, "work/bl-z", "main", "nothing [bl-z]", "[bl-z]").unwrap();
    assert_eq!(tip(&root), "seed");
}

#[test]
fn deliver_surfaces_a_conflict_as_an_error() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("seed.txt"), "from work\n").unwrap();
    Project::run(&wt, &["commit", "-qam", "work edit"]).unwrap();
    // Integration moves the same line — the squash can't merge cleanly.
    fs::write(root.join("seed.txt"), "from main\n").unwrap();
    Project::run(&root, &["commit", "-qam", "main edit"]).unwrap();

    let err = p.deliver(&wt, "work/bl-x", "main", "clash [bl-x]", "[bl-x]").unwrap_err();
    assert!(err.to_string().contains("delivery conflict"));
    // The half-merge was aborted: no MERGE_HEAD pending, the worktree is clean
    // for the agent to reintegrate by hand.
    assert!(!Project::ok(&wt, &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"]).unwrap());
    assert!(Project::ok(&wt, &["diff", "--quiet", "HEAD"]).unwrap());
}

#[test]
fn deliver_skips_when_this_incarnations_delivery_already_landed() {
    // The bl-430e retry: a close squash-delivered, then aborted after the seal
    // (push race) — the squash is BINDING and stands (§14); main keeps the
    // delivery and the branch survives. The re-close must not mint a duplicate.
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    // Commit under a DIFFERENT subject so the squash deterministically mints a
    // distinct sha (capture + squash of the same tree/parent/message in the
    // same second collide — the is-ancestor guard's case, tested below).
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "wip"]).unwrap();
    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    // A concurrent agent lands on main AFTER the delivery, so main and the
    // branch differ again — the empty-deliverable guard alone would re-squash,
    // and `merge-tree` of already-merged work yields main's own tree: an EMPTY
    // duplicate delivery commit (the bl-3bfd outcome).
    fs::write(root.join("other.txt"), "other\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "concurrent work"]).unwrap();

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    assert_eq!(p.marked("main", "[bl-x]").unwrap().len(), 1); // one delivery, no dup
    assert_eq!(tip(&root), "concurrent work"); // the retry minted nothing
}

#[test]
fn deliver_skips_a_branch_already_fully_merged_into_integration() {
    // The sha-collision shape of the bl-430e retry: capture then squash can mint
    // the SAME commit (same parent/tree/message/second), so the surviving
    // delivery IS the branch tip. Every branch commit on integration = nothing
    // to deliver, even once integration moves on (trees differ again).
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    Project::run(&wt, &["add", "-A"]).unwrap();
    Project::run(&wt, &["commit", "-qm", "Add feature [bl-x]"]).unwrap();
    Project::run(&root, &["merge", "-q", "--ff-only", "work/bl-x"]).unwrap(); // the collided delivery
    fs::write(root.join("other.txt"), "other\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "concurrent work"]).unwrap();

    p.deliver(&wt, "work/bl-x", "main", "Add feature [bl-x]", "[bl-x]").unwrap();
    assert_eq!(p.marked("main", "[bl-x]").unwrap().len(), 1);
    assert_eq!(tip(&root), "concurrent work");
}

#[test]
fn marked_returns_the_marked_commits_newest_first() {
    let (tmp, root, p) = project();
    let wt = tmp.path().join("wt");

    // First incarnation of bl-x: deliver onto main, then close it out (discard).
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("a.txt"), "1\n").unwrap();
    p.deliver(&wt, "work/bl-x", "main", "first [bl-x]", "[bl-x]").unwrap();
    p.discard(&wt, "work/bl-x").unwrap();

    // A reused id only begins after the prior closed, so its delivery lands
    // LATER — deliveries are monotonic with incarnations (§11). The second
    // deliver MUST land despite the first `[bl-x]` in history: the
    // retry-idempotence skip (bl-430e) is scoped to commits since this
    // branch forked, and the prior delivery predates the fork.
    p.materialize(&wt, "work/bl-x").unwrap();
    fs::write(wt.join("b.txt"), "2\n").unwrap();
    p.deliver(&wt, "work/bl-x", "main", "second [bl-x]", "[bl-x]").unwrap();

    let shas = p.marked("main", "[bl-x]").unwrap();
    let subject = |sha: &str| Project::run(&root, &["log", "-1", "--format=%s", sha]).unwrap().trim().to_string();
    // Newest first: the k-th-most-recent incarnation maps to the k-th element.
    assert_eq!(shas.iter().map(|s| subject(s)).collect::<Vec<_>>(), ["second [bl-x]", "first [bl-x]"]);
    // A never-delivered id → empty (an honest cross-clone miss, §11).
    assert!(p.marked("main", "[bl-zzzz]").unwrap().is_empty());
}

#[test]
fn changed_task_paths_lists_the_ops_touched_task_file() {
    let (tmp, root, _p) = project();
    fs::create_dir(root.join("tasks")).unwrap();
    fs::write(root.join("tasks/bl-9f9f.md"), "x\n").unwrap();
    Project::run(&root, &["add", "-A"]).unwrap();
    Project::run(&root, &["commit", "-qm", "add task"]).unwrap();
    fs::remove_file(root.join("tasks/bl-9f9f.md")).unwrap(); // the close diff

    assert_eq!(changed_task_paths(&root).unwrap(), ["tasks/bl-9f9f.md"]);
    let _ = tmp;
}

#[test]
fn a_git_failure_surfaces_with_stderr() {
    let outside = TempDir::new().unwrap(); // not a git repo
    let err = changed_task_paths(outside.path()).unwrap_err();
    assert!(err.to_string().starts_with("git diff"));
}

#[test]
fn root_commit_is_the_seed_root_or_none_off_a_non_repo() {
    // bl-1ce7: the canonical, remote-free repo identity. On a one-commit repo
    // the root IS that seed commit; off a non-repo dir it is None (a ball
    // created there records nothing — back-compat).
    let (_tmp, root, p) = project();
    let r = p.root_commit().expect("a committed repo has a root");
    let seed = Project::run(&root, &["rev-parse", "HEAD"]).unwrap().trim().to_string();
    assert_eq!(r, seed, "the sole commit is the root");
    let outside = TempDir::new().unwrap(); // not a git repo
    assert!(Project::at(outside.path()).root_commit().is_none());
}
