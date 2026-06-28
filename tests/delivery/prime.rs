//! The `prime` housekeeping scenarios — that worktrees materialize at CLAIM
//! ONLY (bl-c2bf: prime re-creates nothing), and its §14 rollback decline. A
//! sibling of the [`crate`] harness (same crate, shared helpers), split out for
//! the 300-line cap.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use balls::delivery::worktree_path;
use balls::layout::Xdg;
use tempfile::TempDir;

use crate::{delivery, post, prime, project};

/// Write a `tasks/<id>.md` ball with `claimant` into the store checkout `store`.
fn claimed_ball(store: &Path, id: &str, claimant: &str) {
    let tasks = store.join("tasks");
    fs::create_dir_all(&tasks).unwrap();
    fs::write(
        tasks.join(format!("{id}.md")),
        format!("+++\ntitle = \"t\"\ncreated = 0\nupdated = 0\nclaimant = \"{claimant}\"\n+++\n"),
    )
    .unwrap();
}

#[test]
fn prime_does_not_materialize_a_claimed_worktree() {
    // bl-c2bf: worktrees materialize at CLAIM and nowhere else. Even a ball the
    // actor still holds gets NO worktree from prime (re-priming a lost one is
    // `unclaim` + `claim`), and prime prints no path. This is the fix for the
    // lagging-clone bug: a stale store still reading `claimed` can no longer
    // make a bogus worktree off THIS checkout's `main`.
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap();
    // balls invokes the plugin with cwd at the store checkout (§13 diffless).
    let store = tmp.path().join("store");
    claimed_ball(&store, "bl-mine", "me");

    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let mine = worktree_path(&xdg, "delivery", inv, "bl-mine");

    delivery(&store, &home, "prime", "post", &prime("me", inv))
        .assert()
        .success()
        .stdout(""); // no worktree path surfaces — prime materializes nothing

    assert!(!mine.exists()); // the bogus-worktree bug, closed
}

/// The §7 wire of a rolled-back `prime` (§14): the diffless payload plus the
/// `rolling_back` tag. The unwind invokes it with cwd = the LANDING (`pre_dir`
/// in the engine's unwind), not the store.
fn rollback_prime(actor: &str, invocation: &str) -> String {
    format!(r#"{{"actor":"{actor}","binding":{{"invocation_path":"{invocation}"}},"rolling_back":"post"}}"#)
}

#[test]
fn an_aborted_prime_rolls_back_as_a_clean_no_op() {
    // bl-62eb: prime is an idempotent refresher, so its rollback DECLINES
    // (§14) — no scan, no print, no prune, exit 0. The old path scanned
    // `cwd/tasks` for the claimed set before dispatching, and the unwind's cwd
    // is the LANDING (which has no tasks/), so every aborted prime died with
    // `No such file or directory (os error 2)` instead of declining.
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap();

    // A settled work/<id> branch a FORWARD prime would prune — the rollback
    // must not (declining means not running the deferred cleanup either).
    delivery(&root, &home, "claim", "post", &post(inv, "bl-x", "T")).assert().success();
    delivery(&root, &home, "unclaim", "post", &post(inv, "bl-x", "T")).assert().success();

    let landing = tmp.path().join("landing"); // no tasks/ dir, like the real landing
    fs::create_dir_all(&landing).unwrap();
    delivery(&landing, &home, "prime", "post", &rollback_prime("me", inv)).assert().success().stdout("");

    let branch_exists = Command::new("git")
        .current_dir(&root)
        .args(["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-x"])
        .output()
        .unwrap()
        .status
        .success();
    assert!(branch_exists); // the prune is forward-prime work, not rollback work
}
