//! bl-4a88: the delivery precondition end-to-end. When the invocation path is
//! NOT a git repo, `bl prime --stealth` used to found a tracker whose tasks
//! could never be claimed or closed — the lifecycle died with a raw
//! `fatal: not a git repository` from the first worktree act, deferred until
//! claim. The fix surfaces the one predicate ([`Repo::is_git_repo`]) early, in
//! balls' voice: prime WARNS and no-ops; claim.post / close.pre ABORT cleanly.
//!
//! Driven by subprocess against the real binary, exactly as balls would — so it
//! exercises the binary edge (the gate call + the prime warning) the unit tests
//! cannot reach. tarpaulin ignores `tests/`, hence the unit angles in
//! `delivery_precondition_tests` / `delivery_repo_tests`.

use std::fs;

use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

use crate::{change_dir, delivery, post, pre, prime};

/// The raw git voice the fix REPLACES — it must never reach the user.
const RAW_FATAL: &str = "fatal: not a git repository";

#[test]
fn prime_in_a_non_git_dir_warns_and_no_ops_instead_of_aborting() {
    // The founding moment (`bl prime --stealth` in a non-repo): prime succeeds
    // (exit 0, does NOT refuse), prints nothing, and emits the single
    // balls-voice warning naming both drains — before any task is filed.
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let non_git = tmp.path().join("docs"); // a plain dir, never `git init`ed
    fs::create_dir(&non_git).unwrap();
    let inv = non_git.to_str().unwrap();

    delivery(&non_git, &home, "prime", "post", &prime("me", inv))
        .assert()
        .success()
        .stdout("") // warned + no-op'd: nothing surfaces
        .stderr(contains("is not a git repository"))
        .stderr(contains("git init"))
        .stderr(contains("bl conf remove"))
        .stderr(contains(RAW_FATAL).not()); // the raw git voice is swallowed
}

#[test]
fn claim_in_a_non_git_dir_aborts_cleanly_not_with_a_raw_git_fatal() {
    // The deferred failure the bug surfaced at: claim.post can no longer mint an
    // un-retirable task — it aborts (exit 1) with the clean precondition message.
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let non_git = tmp.path().join("docs");
    fs::create_dir(&non_git).unwrap();
    let inv = non_git.to_str().unwrap();

    delivery(&non_git, &home, "claim", "post", &post(inv, "bl-x", "t"))
        .assert()
        .failure()
        .code(1)
        .stderr(contains("is not a git repository"))
        .stderr(contains("git init"))
        .stderr(contains("bl conf remove"))
        .stderr(contains(RAW_FATAL).not());
}

#[test]
fn close_in_a_non_git_dir_aborts_cleanly_so_the_tracker_is_never_un_drainable() {
    // close is the ONLY retirement; close.pre gates on the same predicate, so the
    // message (which names `git init` AND detaching delivery) is the way out.
    // The cwd is the change worktree (a real repo, where the id is recovered);
    // only the invocation path is the non-repo the gate rejects.
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let non_git = tmp.path().join("docs");
    fs::create_dir(&non_git).unwrap();
    let inv = non_git.to_str().unwrap();
    let change = change_dir(tmp.path(), "change");

    delivery(&change, &home, "close", "pre", &pre(inv, "t"))
        .assert()
        .failure()
        .code(1)
        .stderr(contains("is not a git repository"))
        .stderr(contains(RAW_FATAL).not());
}
