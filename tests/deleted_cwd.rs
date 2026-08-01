//! bl-4787: an op must not care whether its INVOCATION DIRECTORY still exists.
//!
//! `close` removes the `work/<id>` worktree at `close.post` — and that worktree
//! is the natural place to have run the close from, since `claim` prints its
//! path and every edit happens there. Everything balls spawns after that point
//! inherits a deleted cwd, so any child `git` that has to `getcwd()` dies with
//! `fatal: Unable to read current working directory` — noise landing in the one
//! output a caller reads to decide whether their close succeeded, and, when the
//! unread failure was the §9 report's trailer read, the `bl-id`-trailer panic on
//! a close that had already delivered, sealed and retired.
//!
//! Both stories below assert the ABSENCE of that fatal through the freshly-built
//! binary: a close run from inside the worktree it removes, and an op whose
//! invocation directory is already gone before `bl` starts. tarpaulin counts
//! src/ only, so this file is coverage-neutral.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

/// The git fatal this file exists to keep out of balls' output.
const FATAL: &str = "Unable to read current working directory";

/// `bl` rooted in `project`, HOME/`XDG_STATE_HOME` pinned under the tempdir so
/// the store clone never touches the real `$HOME`. The inherited `BALLS_*`
/// recursion bookkeeping is scrubbed — this file itself runs inside a `bl close`
/// gate under the orchestrator, so a top-level `bl` here must start at depth 0.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME");
    cmd
}

/// `git -C <cwd> <args>`, asserting success (plain-git harness setup).
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A verb's one stdout product (create's id, claim's worktree path), trimmed.
fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// A NON-bare project repo seeded on `main`, plus a primed stealth store under
/// the tempdir.
fn project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (home, state, root) = (tmp.join("h"), tmp.join("s"), tmp.join("proj"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&state).unwrap();
    git(tmp, &["init", "-q", "-b", "main", &root.to_string_lossy()]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "user.email", "t@t"]);
    fs::write(root.join("seed.txt"), "seed\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "seed"]);
    bl(&root, &home, &state).arg("prime").assert().success();
    (root, home, state)
}

#[test]
fn a_close_run_from_inside_the_work_worktree_it_removes_emits_no_git_fatal() {
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let id = stdout(bl(&root, &home, &state).args(["create", "Add feature", "--as", "me"]).assert().success());
    let wt = PathBuf::from(stdout(
        bl(&root, &home, &state).args(["claim", &id, "--as", "me"]).assert().success(),
    ));
    fs::write(wt.join("feature.txt"), "shipped\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-qm", &format!("work [{id}]")]);

    // The close runs WITH THE WORKTREE AS ITS CWD — `-C` addresses the store, so
    // the only thing the cwd contributes is that `close.post` deletes it
    // mid-run. It still delivers, and says so without a git fatal.
    let out = bl(&wt, &home, &state)
        .args(["-C", &root.to_string_lossy(), "close", &id, "--as", "me"])
        .assert()
        .success()
        .stderr(contains(FATAL).not())
        .stderr(contains("panicked").not());
    assert!(String::from_utf8_lossy(&out.get_output().stderr).contains(&format!("close {id}")));
    assert!(!wt.exists(), "the close removed the worktree it ran in");
}

#[test]
fn an_op_whose_invocation_directory_is_already_gone_still_seals() {
    // The same wound with the timing removed: `bl` starts life in a directory
    // that no longer exists. Nothing balls asks git to do depends on it.
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let doomed = tmp.path().join("doomed");
    fs::create_dir(&doomed).unwrap();
    let bin = assert_cmd::cargo::cargo_bin("bl");

    // `sh` deletes its own cwd, then execs `bl` into it — the only way to hand a
    // child a cwd that is already gone (a spawn-time `chdir` would fail).
    let id = stdout(
        Command::new("sh")
            .current_dir(&doomed)
            .args(["-c", r#"rmdir "$PWD"; exec "$0" -C "$1" create "From nowhere" --as me"#])
            .arg(&bin)
            .arg(root.to_string_lossy().to_string())
            .env("HOME", &home)
            .env("XDG_STATE_HOME", &state)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("BALLS_PLUGIN_DEPTH")
            .env_remove("BALLS_PLUGIN_NAME")
            .assert()
            .success()
            .stderr(contains(FATAL).not())
            .stderr(contains("panicked").not()),
    );
    // Sealed for real: the trailer block survived, so the ball is addressable.
    bl(&root, &home, &state).args(["show", &id]).assert().success().stdout(contains("From nowhere"));
}
