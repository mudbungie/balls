//! Init-related stories: 1, 2, 3, 73, 74, 75. Stealth-mode and
//! `--tasks-dir` stories live in `init_stealth.rs`.

mod common;

// The raw `git` subprocesses in `fresh_install_no_git_identity` bypass
// `clean_git_command`, so they must scrub the inherited git env themselves.
// The `bl` binary needs no such scrub — it routes every git subprocess
// through `clean_git_command`, which clears these on the production path.
use balls::git::GIT_ENV_VARS;
use common::*;
use predicates::prelude::*;

#[test]
fn story_1_init_in_existing_git_repo() {
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    assert!(repo.path().join(".balls/config.json").exists());
    assert!(repo.path().join(".balls/tasks").exists());
    assert!(repo.path().join(".balls/local/claims").exists());
    assert!(repo.path().join(".balls/local/lock").exists());
    let gi = std::fs::read_to_string(repo.path().join(".gitignore")).unwrap();
    assert!(gi.contains(".balls/local"));
    assert!(gi.contains(".balls-worktrees"));
    // bl-c439: the master_url state-repo clone is gitignored
    // unconditionally, same as the code-refs cache.
    assert!(gi.contains(".balls/code-refs"));
    assert!(gi.contains(".balls/state-repo"));
    let log = git(repo.path(), &["log", "--oneline"]);
    assert!(log.contains("balls: initialize"));
}

#[test]
fn story_2_init_twice_is_idempotent() {
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();
    bl(repo.path()).arg("init").assert().success();
}

#[test]
fn story_3_init_in_cloned_repo_creates_local_only() {
    let remote = new_bare_remote();
    let dev_a = clone_from_remote(remote.path(), "alice");
    bl(dev_a.path()).arg("init").assert().success();
    push(dev_a.path());

    let _id = create_task(dev_a.path(), "task from A");
    push(dev_a.path());

    let dev_b = clone_from_remote(remote.path(), "bob");
    // Fresh clone has no .balls/tasks symlink and no .balls/local yet —
    // they're per-clone, gitignored, and materialized by `bl init`.
    assert!(!dev_b.path().join(".balls/tasks").exists());
    assert!(!dev_b.path().join(".balls/local").exists());
    bl(dev_b.path()).arg("init").assert().success();
    assert!(dev_b.path().join(".balls/local/claims").exists());
    assert!(dev_b.path().join(".balls/tasks").is_symlink());
    assert!(dev_b.path().join(".balls/worktree").exists());
}

#[test]
fn story_73_init_in_repo_with_no_commits() {
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@t"]);
    git(dir.path(), &["config", "user.name", "t"]);
    bl(dir.path()).arg("init").assert().success();
    assert!(dir.path().join(".balls/config.json").exists());
}

#[test]
fn story_74_outside_git_repo() {
    let dir = tmp();
    bl(dir.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not a git repository"));
}

#[test]
fn story_75_not_initialized() {
    let repo = new_repo();
    bl(repo.path())
        .args(["list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not initialized"));
}

/// Regression: bl must work on a system with no git identity configured,
/// neither globally nor in the local repo. Previously git_ensure_user()
/// only ran inside Store::init(), so post-init commands (create, claim,
/// review, ...) hit `git commit` with no identity and failed (or, in CI
/// hooks that swallow stderr, hung waiting on a prompt).
#[test]
fn fresh_install_no_git_identity() {
    use assert_cmd::Command;
    use std::process::Command as StdCommand;

    let home = tmp();
    let dir = tmp();

    // Initialize a git repo without configuring any user.email/user.name.
    // Crucially, we also point HOME at an empty dir and silence any
    // global/system gitconfig the test machine may have, so we truly
    // simulate a fresh box.
    let mut g = StdCommand::new("git");
    g.current_dir(dir.path())
        .args(["init", "-q", "-b", "main"])
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for var in GIT_ENV_VARS {
        g.env_remove(var);
    }
    assert!(g.output().expect("git init").status.success());

    let bl_fresh = || {
        let mut c = Command::cargo_bin("bl").unwrap();
        c.current_dir(dir.path())
            .env("BALLS_IDENTITY", "test-user")
            .env("HOME", home.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        c
    };

    bl_fresh().arg("init").assert().success();

    // Clear any local identity that `bl init` set. This proves the fix:
    // `Store::discover()` must re-seed identity on every command path,
    // not rely on init having done it once. Simulates a fresh system,
    // a wiped repo config, or any path where init's seed didn't stick.
    let mut clear_email = StdCommand::new("git");
    clear_email
        .current_dir(dir.path())
        .args(["config", "--local", "--unset", "user.email"])
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for var in GIT_ENV_VARS {
        clear_email.env_remove(var);
    }
    let _ = clear_email.output();
    let mut clear_name = StdCommand::new("git");
    clear_name
        .current_dir(dir.path())
        .args(["config", "--local", "--unset", "user.name"])
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for var in GIT_ENV_VARS {
        clear_name.env_remove(var);
    }
    let _ = clear_name.output();

    bl_fresh().args(["create", "fresh task"]).assert().success();

    // Find the new task id and exercise the rest of the lifecycle so
    // every command path runs against a repo whose only identity comes
    // from git_ensure_user on discover.
    let ready = bl_fresh().args(["ready", "--json"]).output().unwrap();
    assert!(ready.status.success());
    let stdout = String::from_utf8_lossy(&ready.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let id = v[0]["id"].as_str().expect("ready task id").to_string();

    bl_fresh().args(["claim", &id]).assert().success();
    bl_fresh()
        .args(["update", &id, "--note", "progress"])
        .assert()
        .success();
    bl_fresh()
        .args(["review", &id, "-m", "fresh review"])
        .assert()
        .success();
}
