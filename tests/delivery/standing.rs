//! §11/§14 delivery STANDING, message, and no-op semantics through the real
//! `bl-delivery` binary — the [`crate`] harness's subprocess pattern.
//!
//! Covers the four standings a close/prime must tell apart:
//!   * DIVERGED (bl-c231/bl-65e0): a `[id]` delivery stands AND the surviving
//!     `work/<id>` branch grew a commit beyond it — a re-close ABORTS loudly and
//!     prime's prune PRESERVES the branch (never a silent skip that strands it).
//!   * the multi-commit `-m` squash body — subject, then `-m` narration, then the
//!     work messages oldest-first (bl-b9a6/bl-9961).
//!   * the empty-deliverable / never-claimed / discarded-branch NO-OPS.
//!
//! ORACLE CORRECTION: the assignment predicted an empty deliverable "lands a
//! bare tagged no-diff commit". The real binary does the opposite — with no
//! tree diff since the fork, `deliver`'s standing is SETTLED (the branch is an
//! ancestor of integration) and the squash is skipped, so integration is left
//! untouched (mirrors the src unit `deliver_is_a_no_op_for_an_empty_deliverable`).
//! These tests assert the real no-op, not the predicted commit.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use balls::delivery_path::worktree_path;
use balls::layout::Xdg;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

use crate::{change_dir, delivery, git, post, pre, prime, project};

/// The `main` tip subject of the project repo at `root`.
fn subject(root: &Path) -> String {
    let out = Command::new("git").current_dir(root).args(["log", "-1", "--format=%s", "main"]).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// The full `main` tip message body of the project repo at `root`.
fn body(root: &Path) -> String {
    let out = Command::new("git").current_dir(root).args(["log", "-1", "--format=%B", "main"]).output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// How many `[bl-x]`-tagged commits stand on `main` — the no-duplicate invariant.
fn marked_count(root: &Path) -> String {
    let out = Command::new("git")
        .current_dir(root)
        .args(["rev-list", "--count", "--fixed-strings", "--grep=[bl-x]", "main"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Does `refs/heads/work/bl-x` exist in the project repo at `root`?
fn work_branch_exists(root: &Path) -> bool {
    Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-x"])
        .output()
        .unwrap()
        .status
        .success()
}

/// A HOME under a fresh tempdir + the project repo + its invocation string.
fn arena(tmp: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let home = tmp.join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp);
    (home, root)
}

#[test]
fn a_diverged_close_aborts_and_prime_preserves_the_surviving_branch() {
    // bl-c231/bl-65e0: A delivers, the branch survives (close.post keeps it),
    // then it grows a commit BEYOND the delivery. A re-close must ABORT loudly
    // ("already delivered … file a new task"), never silently skip and strand
    // the extra work — and prime's prune must PRESERVE the diverged branch.
    let tmp = TempDir::new().unwrap();
    let (home, root) = arena(tmp.path());
    let inv = root.to_str().unwrap();
    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let wt = worktree_path(&xdg, "delivery", inv, "bl-x");

    // Deliver once, then tear the worktree down (branch survives). Commit the
    // work under a message DISTINCT from the delivery subject so the branch
    // commit's SHA differs from the squash's — otherwise same tree+parent+
    // message+time collide the two and the branch reads as an ancestor of main
    // (no divergence, the git-SHA-coincidence trap).
    delivery(&root, &home, "claim", "post", &post(inv, "bl-x", "Add feature")).assert().success();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "wip"]);
    delivery(&change_dir(tmp.path(), "c1"), &home, "close", "pre", &pre(inv, "Add feature")).assert().success();
    delivery(&root, &home, "close", "post", &post(inv, "bl-x", "Add feature")).assert().success();
    assert_eq!(subject(&root), "Add feature [bl-x]");
    assert!(work_branch_exists(&root));

    // Re-materialize the surviving branch and commit MORE beyond the delivery
    // (the bl-65e0 handoff) — again a distinct message, clean tree at close.
    delivery(&root, &home, "claim", "post", &post(inv, "bl-x", "Add feature")).assert().success();
    fs::write(wt.join("more.txt"), "beyond\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "more work"]);

    // The re-close ABORTS: the delivery stands, the branch has diverged.
    delivery(&change_dir(tmp.path(), "c2"), &home, "close", "pre", &pre(inv, "Add feature"))
        .assert()
        .failure()
        .code(1)
        .stderr(contains("already delivered").and(contains("file a new task")));

    // Exactly one delivery still stands — no duplicate, integration unmoved.
    assert_eq!(subject(&root), "Add feature [bl-x]");
    assert_eq!(marked_count(&root), "1");

    // prime's prune PRESERVES the diverged branch (only settled ones are cut).
    let store = tmp.path().join("store");
    fs::create_dir_all(store.join("tasks")).unwrap();
    delivery(&store, &home, "prime", "post", &prime("me", inv)).assert().success();
    assert!(work_branch_exists(&root), "prime pruned a diverged branch");
}

/// A close.pre wire carrying a `-m` note (§7 `command.message`), id read back
/// from the change worktree's staged deletion (no `bl-id` on the pre wire).
fn pre_m(invocation: &str, title: &str, note: &str) -> String {
    format!(
        r#"{{"binding":{{"invocation_path":"{invocation}"}},"current_state":{{"title":"{title}"}},"command":{{"message":"{note}"}}}}"#
    )
}

#[test]
fn close_m_squash_body_is_subject_then_narration_then_work_oldest_first() {
    // bl-b9a6/bl-9961: the subject is ALWAYS the tagged ball title; the `-m`
    // note and the author's work messages BOTH live in the body under it,
    // narration first, then every non-merge work commit oldest-first.
    let tmp = TempDir::new().unwrap();
    let (home, root) = arena(tmp.path());
    let inv = root.to_str().unwrap();
    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let wt = worktree_path(&xdg, "delivery", inv, "bl-x");

    delivery(&root, &home, "claim", "post", &post(inv, "bl-x", "T")).assert().success();
    // Two real work commits on the branch (clean tree at close, so capture adds
    // nothing and the body is exactly subject + narration + these two).
    fs::write(wt.join("a.txt"), "a\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "first work"]);
    fs::write(wt.join("b.txt"), "b\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "second work"]);

    delivery(&change_dir(tmp.path(), "c"), &home, "close", "pre", &pre_m(inv, "T", "close narration"))
        .assert()
        .success();

    assert_eq!(subject(&root), "T [bl-x]");
    assert_eq!(body(&root), "T [bl-x]\n\nclose narration\n\nfirst work\n\nsecond work");
}

#[test]
fn an_empty_deliverable_close_is_a_no_op_not_a_bare_commit() {
    // Claimed, never worked: the branch sits at the fork with no tree diff, so
    // `deliver`'s standing is SETTLED and the squash is skipped — integration is
    // untouched. (The predicted "bare tagged no-diff commit" never lands.)
    let tmp = TempDir::new().unwrap();
    let (home, root) = arena(tmp.path());
    let inv = root.to_str().unwrap();

    delivery(&root, &home, "claim", "post", &post(inv, "bl-x", "Empty")).assert().success();
    assert!(work_branch_exists(&root));
    delivery(&change_dir(tmp.path(), "c"), &home, "close", "pre", &pre(inv, "Empty")).assert().success();

    assert_eq!(subject(&root), "seed"); // integration untouched — no delivery commit
    assert_eq!(marked_count(&root), "0");
}

#[test]
fn a_never_claimed_close_is_a_no_op_no_branch_no_delivery() {
    // Closing an epic that was never claimed: no `work/<id>` branch exists, so
    // `deliver` returns early — a clean no-op, integration unmoved.
    let tmp = TempDir::new().unwrap();
    let (home, root) = arena(tmp.path());
    let inv = root.to_str().unwrap();

    assert!(!work_branch_exists(&root));
    delivery(&change_dir(tmp.path(), "c"), &home, "close", "pre", &pre(inv, "Epic")).assert().success();

    assert_eq!(subject(&root), "seed");
    assert_eq!(marked_count(&root), "0");
}

#[test]
fn a_branch_dash_d_discard_makes_the_close_a_clean_no_op() {
    // The bl-65e0 discard remedy: unclaim leaves committed-but-undelivered work
    // on the branch, a human `git branch -D`s it away, and a later close is a
    // clean no-op — the branch is gone, so `deliver` lands nothing.
    let tmp = TempDir::new().unwrap();
    let (home, root) = arena(tmp.path());
    let inv = root.to_str().unwrap();
    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let wt = worktree_path(&xdg, "delivery", inv, "bl-x");

    delivery(&root, &home, "claim", "post", &post(inv, "bl-x", "WIP")).assert().success();
    fs::write(wt.join("kept.txt"), "undelivered\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", "wip"]);
    // unclaim tears the worktree down but keeps the branch; then discard it.
    delivery(&root, &home, "unclaim", "post", &post(inv, "bl-x", "WIP")).assert().success();
    assert!(work_branch_exists(&root));
    git(&root, &["branch", "-D", "work/bl-x"]);
    assert!(!work_branch_exists(&root));

    delivery(&change_dir(tmp.path(), "c"), &home, "close", "pre", &pre(inv, "WIP")).assert().success();

    assert_eq!(subject(&root), "seed"); // no delivery commit
    assert_eq!(marked_count(&root), "0");
}
