//! The `prime` housekeeping scenarios — that worktrees materialize at CLAIM
//! ONLY (bl-c2bf: prime re-creates nothing), and its §14 rollback decline. A
//! sibling of the [`crate`] harness (same crate, shared helpers), split out for
//! the 300-line cap.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use balls::delivery_path::worktree_path;
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

#[test]
fn prime_reports_an_unsettled_branch_whose_worktree_is_gone() {
    // bl-c117 (bl-18bf piece 3): unclaim tore the worktree down but the branch
    // carries a committed-yet-undelivered diff — prime must SAY so on stderr,
    // naming both remedies, and prune nothing. Then bl-baa0: with the ball
    // CLOSED (its task file gone from the very store checkout core runs
    // prime.post in) the same debris reports the discard arm ALONE. This is the
    // one place the cwd-IS-the-store wiring is proven end to end.
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let root = project(tmp.path());
    let inv = root.to_str().unwrap();

    let xdg = Xdg::with(&home, None, Some(home.join("state").to_str().unwrap()));
    let wt = worktree_path(&xdg, "delivery", inv, "bl-gone");

    delivery(&root, &home, "claim", "post", &post(inv, "bl-gone", "T")).assert().success();
    fs::write(wt.join("kept.txt"), "undelivered\n").unwrap();
    let g = |args: &[&str]| Command::new("git").current_dir(&wt).args(args).assert().success();
    g(&["add", "-A"]);
    g(&["commit", "-qm", "wip"]);
    delivery(&root, &home, "unclaim", "post", &post(inv, "bl-gone", "T")).assert().success();
    assert!(!wt.exists());

    let store = tmp.path().join("store");
    claimed_ball(&store, "bl-gone", "me"); // still OPEN: both remedies stand
    delivery(&store, &home, "prime", "post", &prime("me", inv)).assert().success().stderr(
        "bl-delivery: work/bl-gone is committed but its worktree is gone — bl claim bl-gone \
         re-materializes onto it (a later close still delivers, bl-65e0), \
         or discard with git branch -D work/bl-gone\n",
    );

    // Close the ball the only way §10 records it: the task file goes away.
    fs::remove_file(store.join("tasks").join("bl-gone.md")).unwrap();
    delivery(&store, &home, "prime", "post", &prime("me", inv)).assert().success().stderr(
        "bl-delivery: work/bl-gone is committed but its worktree is gone, and bl-gone is closed \
         (no task file — absence is the record), so nothing can re-claim or deliver it: \
         its content is NOT contained in main — read it with git diff main...work/bl-gone, \
         then discard with git branch -D work/bl-gone\n",
    );

    // Reported, never pruned: the branch — the only copy of the diff — survives.
    let branch_exists = Command::new("git")
        .current_dir(&root)
        .args(["rev-parse", "--verify", "--quiet", "refs/heads/work/bl-gone"])
        .output()
        .unwrap()
        .status
        .success();
    assert!(branch_exists);
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
