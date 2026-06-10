//! The `prime` re-materialization scenarios (§11/§12) — the per-session scan
//! over the actor's still-claimed balls, its idempotence, and its §14 rollback
//! decline. A sibling of the [`crate`] harness (same crate, shared helpers),
//! split out for the 300-line cap.

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
fn prime_re_materializes_only_the_actors_still_claimed_worktrees() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap();
    // balls invokes the plugin with cwd at the store checkout (§13 diffless), so
    // prime scans the store from the cwd, not a wire field.
    let store = tmp.path().join("store");
    claimed_ball(&store, "bl-mine", "me");
    claimed_ball(&store, "bl-theirs", "you"); // another actor — left alone

    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let mine = worktree_path(&xdg, "delivery", inv, "bl-mine");
    let theirs = worktree_path(&xdg, "delivery", inv, "bl-theirs");

    // The path of each re-materialized worktree prints — the resume-session
    // counterpart of claim.post's print (§11) — and only mine.
    delivery(&store, &home, "prime", "post", &prime("me", inv))
        .assert()
        .success()
        .stdout(format!("{}\n", mine.display()));

    assert!(mine.join("seed.txt").exists()); // my claim re-materialized
    assert!(!theirs.exists()); // a different actor's claim is not mine to make

    // Idempotent: a second prime over the same set converges to a no-op (the
    // path still prints — prime re-surfaces it every session).
    delivery(&store, &home, "prime", "post", &prime("me", inv)).assert().success().stdout(format!("{}\n", mine.display()));
    assert!(mine.join("seed.txt").exists());
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
