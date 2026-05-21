//! Shared fixtures for `delivery_tests` and `delivery_close_remote_tests`.
//! Lives in its own module so both files reuse one definition and stay
//! under the 300-line limit.

use crate::task::{NewTaskOpts, Task};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

pub(crate) fn empty_task() -> Task {
    Task::new(
        NewTaskOpts {
            title: "t".into(),
            ..Default::default()
        },
        "bl-abcd".into(),
    )
}

pub(crate) fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed: {out:?}");
}

/// Stand up a tiny git repo with a single tagged commit so the local
/// resolve path has something real to find. Returns (root, sha).
pub(crate) fn local_repo_with_tag(id: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@e.x"]);
    git(dir.path(), &["config", "user.name", "t"]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    git(dir.path(), &["commit", "-qm", &format!("seed [{id}]")]);
    let sha = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    (dir, sha)
}
