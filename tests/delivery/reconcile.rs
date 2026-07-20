//! bl-22dd — reconcile heals the integration checkout after delivery.
//!
//! The squash lands via `commit-tree` + `update-ref` plumbing, which moves the
//! integration ref WITHOUT syncing the index + working tree of the checkout
//! that owns it — leaving the whole delivered diff as a phantom *staged* change
//! in the user's primary checkout. `Project::reconcile` restores that fourth
//! effect. Both delivery legs run it: the FRESH squash (after the ref-flip) and
//! the `Standing::Settled` skip (a retry / crash between the ref-flip and the
//! sync). This sibling asserts the observable cure end-to-end on the real
//! binary — after each leg the root integration checkout, sitting one commit
//! behind, reports NOTHING dirty and NOTHING staged.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use balls::delivery_path::worktree_path;
use balls::layout::Xdg;
use tempfile::TempDir;

use crate::{change_dir, delivery, git, post, pre, project};

/// stdout of `git <args>` in `cwd`, asserting the command succeeds.
fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8(out.stdout).unwrap()
}

/// The bl-22dd cure, observed: nothing tracked-dirty, nothing staged.
fn assert_no_phantom(root: &Path) {
    assert_eq!(git_out(root, &["status", "--porcelain"]), "", "phantom in `git status --porcelain`");
    assert_eq!(git_out(root, &["diff", "--cached"]), "", "phantom `git diff --cached`");
}

#[test]
fn reconcile_heals_the_root_checkout_after_fresh_and_settled_close() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    // `root` is the integration checkout: a non-bare repo sitting on `main`, the
    // one the squash moves out from under via plumbing.
    let root = project(tmp.path());
    let inv = root.to_str().unwrap();
    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let wt = worktree_path(&xdg, "delivery", inv, "bl-x");

    // claim + work + a FRESH close.pre: the squash update-ref advances `main`
    // under the root checkout, whose index + worktree stay at the parent tree.
    delivery(&root, &home, "claim", "post", &post(inv, "bl-x", "Add feature")).assert().success();
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    let change = change_dir(tmp.path(), "change");
    delivery(&change, &home, "close", "pre", &pre(inv, "Add feature")).assert().success();

    // Fresh-squash path: reconcile ran right after the ref-flip (§14) — the
    // phantom staged diff never survives into the user's checkout.
    assert_no_phantom(&root);

    // Manufacture the "crash between the ref-flip and the sync" state the
    // Settled path is built to heal: pin the root checkout's index + worktree
    // one commit behind the ref (HEAD stays at the delivery), exactly what a
    // missed reconcile leaves — the phantom, live.
    git(&root, &["restore", "--source=HEAD^", "--staged", "--worktree", ":/"]);
    assert_ne!(
        git_out(&root, &["status", "--porcelain"]),
        "",
        "setup: the phantom staged diff must be present before the settled retry"
    );

    // A retry close: the delivery already stands, so `standing` reports
    // Standing::Settled and the squash is SKIPPED — yet reconcile still fires,
    // healing the checkout instead of leaving the phantom.
    let change2 = change_dir(tmp.path(), "change2");
    delivery(&change2, &home, "close", "pre", &pre(inv, "Add feature")).assert().success();

    // Settled skip path: no duplicate squash (the delivery is unchanged) AND no
    // phantom — reconcile is the whole product of the retry.
    assert_no_phantom(&root);
    let subjects = git_out(&root, &["log", "--format=%s", "main"]);
    assert_eq!(subjects.matches("Add feature [bl-x]").count(), 1, "settled retry minted a duplicate squash");
}
