//! Shared scaffolding for `state_repo` test modules. Stands up a bare
//! local repo with a seeded `balls/tasks` ref so `state_repo::ensure`
//! exercises its online branch without a network dependency.

use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

pub(super) fn run(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed: {out:?}");
}

/// Spin up a bare repo with `balls/tasks` seeded to act as a hub URL.
pub(super) fn hub_repo() -> TempDir {
    let scratch = TempDir::new().unwrap();
    let work = scratch.path().join("work");
    let bare = scratch.path().join("hub.git");
    std::fs::create_dir_all(&work).unwrap();
    run(&work, &["init", "-q", "--initial-branch", "balls/tasks"]);
    run(&work, &["config", "user.email", "t@e.x"]);
    run(&work, &["config", "user.name", "t"]);
    std::fs::write(work.join("seed"), "seed\n").unwrap();
    run(&work, &["add", "seed"]);
    run(&work, &["commit", "-qm", "seed"]);
    run(
        scratch.path(),
        &["clone", "--bare", "-q", work.to_str().unwrap(), bare.to_str().unwrap()],
    );
    let kept = TempDir::new().unwrap();
    let dest = kept.path().join("hub.git");
    run(
        scratch.path(),
        &["clone", "--bare", "-q", bare.to_str().unwrap(), dest.to_str().unwrap()],
    );
    kept
}
