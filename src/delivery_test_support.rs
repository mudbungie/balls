//! Shared fixtures for `delivery_tests` and `delivery_close_remote_tests`.
//! Lives in its own module so both files reuse one definition and stay
//! under the 300-line limit.

use crate::git_test_support::{git_run, git_stdout};
use crate::task::{NewTaskOpts, Task};
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

/// Stand up a tiny git repo with a single tagged commit so the local
/// resolve path has something real to find. Returns (root, sha).
pub(crate) fn local_repo_with_tag(id: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    git_run(dir.path(), &["init", "-q", "-b", "main"]);
    git_run(dir.path(), &["config", "user.email", "t@e.x"]);
    git_run(dir.path(), &["config", "user.name", "t"]);
    git_run(dir.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    git_run(dir.path(), &["add", "a.txt"]);
    git_run(dir.path(), &["commit", "-qm", &format!("seed [{id}]")]);
    let sha = git_stdout(dir.path(), &["rev-parse", "HEAD"]);
    (dir, sha)
}
