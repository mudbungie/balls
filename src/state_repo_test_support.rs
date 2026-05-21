//! Shared scaffolding for `state_repo` tests: `Address` constructors,
//! a reachable bare tracker, and a legacy `.balls/worktree` project to
//! exercise the in-place migration path — all without a network.

use crate::git_state;
use crate::git_test_support::git_run;
use crate::tracker_address::Address;
use tempfile::TempDir;

/// An explicit `Address` pointing at `url` — hard-fails first contact.
pub(super) fn explicit(url: &str) -> Address {
    Address { url: Some(url.to_string()), branch: "balls/tasks".into(), explicit: true }
}

/// A non-explicit `Address` for `url` — local-fallbacks an unreachable
/// first contact (a legacy `state_remote`, or the implicit origin).
pub(super) fn implicit_url(url: &str) -> Address {
    Address { url: Some(url.to_string()), branch: "balls/tasks".into(), explicit: false }
}

/// The implicit-default `Address`: no url, offline-bootstrappable.
pub(super) fn implicit() -> Address {
    Address { url: None, branch: "balls/tasks".into(), explicit: false }
}

/// A bare repo carrying a seeded `balls/tasks` ref — a reachable
/// tracker. Returns the temp dir; `hub_url` gives its clone URL.
pub(super) fn hub_repo() -> TempDir {
    let scratch = TempDir::new().unwrap();
    let work = scratch.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    git_run(&work, &["init", "-q", "--initial-branch", "balls/tasks"]);
    git_run(&work, &["config", "user.email", "t@e.x"]);
    git_run(&work, &["config", "user.name", "t"]);
    std::fs::write(work.join("seed"), "seed\n").unwrap();
    git_run(&work, &["add", "seed"]);
    git_run(&work, &["commit", "-qm", "seed", "--no-verify"]);
    let kept = TempDir::new().unwrap();
    let dest = kept.path().join("hub.git");
    git_run(
        scratch.path(),
        &["clone", "--bare", "-q", work.to_str().unwrap(), dest.to_str().unwrap()],
    );
    kept
}

/// Clone URL of a `hub_repo` temp dir.
pub(super) fn hub_url(hub: &TempDir) -> String {
    hub.path().join("hub.git").to_string_lossy().into_owned()
}

/// A bare repo with NO `balls/tasks` ref — a reachable but empty
/// tracker (first-federation seed case).
pub(super) fn empty_hub() -> TempDir {
    let kept = TempDir::new().unwrap();
    let dest = kept.path().join("hub.git");
    git_run(kept.path(), &["init", "-q", "--bare", dest.to_str().unwrap()]);
    kept
}

/// A legacy standalone repo: `balls/tasks` lives in its own git with a
/// `.balls/worktree` checkout carrying one task — the migration source.
pub(super) fn legacy_project() -> TempDir {
    let d = TempDir::new().unwrap();
    let p = d.path();
    git_run(p, &["init", "-q", "-b", "main"]);
    git_run(p, &["config", "user.email", "t@e.x"]);
    git_run(p, &["config", "user.name", "t"]);
    std::fs::write(p.join("code.txt"), "x\n").unwrap();
    git_run(p, &["add", "code.txt"]);
    git_run(p, &["commit", "-qm", "code", "--no-verify"]);
    git_state::create_orphan_branch(p, "balls/tasks", "balls state").unwrap();
    git_run(p, &["worktree", "add", "-q", ".balls/worktree", "balls/tasks"]);
    let wt = p.join(".balls/worktree");
    std::fs::create_dir_all(wt.join(".balls/tasks")).unwrap();
    std::fs::write(wt.join(".balls/tasks/bl-legacytask.json"), "{}\n").unwrap();
    git_run(&wt, &["add", "-A"]);
    git_run(&wt, &["commit", "-qm", "legacy task", "--no-verify"]);
    d
}
