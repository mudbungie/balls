//! One shared `git` helper for the whole unit-test suite.
//!
//! Every test that shells out to `git` against a tempdir fixture must
//! scrub the repo-local `GIT_*` environment first. A `cargo test` run
//! launched from a git hook inherits `GIT_DIR`, `GIT_INDEX_FILE`,
//! `GIT_WORK_TREE` & friends; those override `current_dir`, so an
//! unscrubbed `git init`/`add`/`commit` retargets the *host* repo
//! instead of the fixture — corrupting the live worktree (bl-d1db).
//!
//! Built on `git::clean_git_command`, so the scrub set is defined in
//! exactly one place and shared with the production git path. Test
//! modules call `git_run`/`git_stdout` instead of rolling their own.

use crate::git::clean_git_command;
use std::path::Path;
use std::process::Output;

/// Spawn scrubbed `git` in `dir` and assert it exited successfully —
/// a failed fixture command is a broken test, not a path under test.
fn checked(dir: &Path, args: &[&str]) -> Output {
    let out = clean_git_command(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(out.status.success(), "git {args:?} failed: {out:?}");
    out
}

/// Run `git` in `dir`, asserting success. For fixture setup.
pub(crate) fn git_run(dir: &Path, args: &[&str]) {
    checked(dir, args);
}

/// Run `git` in `dir`, asserting success; return trimmed stdout.
pub(crate) fn git_stdout(dir: &Path, args: &[&str]) -> String {
    String::from_utf8(checked(dir, args).stdout)
        .expect("git stdout is utf-8")
        .trim()
        .to_string()
}
