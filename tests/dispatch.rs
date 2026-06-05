//! End-to-end harness: build the `bl` binary and run it from a throwaway temp
//! directory, never against the dev repo's own task list. Later phases grow a
//! `git init`'d balls repo in this temp dir and assert on real behavior; for
//! the skeleton, the binary only resolves a verb to its §8 op plan.

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// The freshly-built `bl`, pinned to run inside an isolated temp dir.
fn bl(workspace: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(workspace.path());
    cmd
}

#[test]
fn dispatches_a_known_verb_to_its_op_plan() {
    let workspace = TempDir::new().unwrap();
    bl(&workspace)
        .arg("close")
        .assert()
        .success()
        .stdout(contains("close: author -> pre -> seal -> post -> teardown"));
}

#[test]
fn a_diffless_verb_skips_the_seal() {
    let workspace = TempDir::new().unwrap();
    bl(&workspace)
        .arg("show")
        .assert()
        .success()
        .stdout(contains("show: pre -> post"));
}

#[test]
fn an_unknown_verb_exits_with_a_usage_error() {
    let workspace = TempDir::new().unwrap();
    bl(&workspace)
        .arg("frobnicate")
        .assert()
        .failure()
        .code(2)
        .stderr(contains("usage: bl <verb>"));
}
