//! Stories 1, 3, 73–75 rewritten for SPEC-clone-layout (Phase 1B,
//! bl-e802). `bl init` writes only XDG paths now — no `.balls/` at the
//! clone root, no `.gitignore` insertion, no `balls: initialize` on
//! `main`. Stealth-mode stories live in `init_stealth.rs`; the
//! conformance gates for these behaviors live in
//! `tests/conformance_xdg_init.rs`. This file is the human-readable
//! story sibling — same shape, different vocabulary.

mod common;

// The raw `git` subprocesses in `fresh_install_no_git_identity` bypass
// `clean_git_command`, so they must scrub the inherited git env themselves.
// The `bl` binary needs no such scrub — it routes every git subprocess
// through `clean_git_command`, which clears these on the production path.
use balls::encoding::{canonicalize_origin, percent_encode_component, ENC_BALLS_TASKS};
use balls::git::GIT_ENV_VARS;
use balls::xdg_paths::{own_tracker_checkout, XdgBases};
use common::*;
use predicates::prelude::*;

#[test]
fn story_1_init_in_existing_git_repo() {
    let repo = new_repo();
    bl(repo.path()).arg("init").assert().success();

    // SPEC §14.1: no `.balls/` at the clone root, no `.gitignore`
    // touched, no balls-attributed commit on `main`. The clone is
    // pristine; XDG state lives entirely under `HOME`.
    assert!(!repo.path().join(".balls").exists());
    let gi = repo.path().join(".gitignore");
    if gi.exists() {
        let s = std::fs::read_to_string(&gi).unwrap();
        assert!(!s.contains(".balls"), "main .gitignore must stay balls-free: {s}");
    }
    let log = git(repo.path(), &["log", "main", "--format=%s"]);
    for line in log.lines() {
        assert!(!line.starts_with("balls:"), "balls: commit on main: {line}");
    }

    // Per-clone XDG dirs materialize under HOME.
    assert!(claims_dir(repo.path()).exists());
    assert!(lock_dir(repo.path()).exists());
    // Tracker checkout (own branch in the solo case) holds repo.json +
    // project.json, no tracker.json, and the seeded tasks/ scaffold.
    let bases = XdgBases::with(&test_home_path(), None, None, None);
    let url = git(repo.path(), &["remote", "get-url", "origin"])
        .trim()
        .to_string();
    let enc = percent_encode_component(&canonicalize_origin(&url));
    let own = own_tracker_checkout(&bases, &enc);
    assert!(own.join(".balls/repo.json").exists());
    assert!(own.join(".balls/project.json").exists());
    assert!(!own.join(".balls/tracker.json").exists());
    assert!(own.join(".balls/tasks").is_dir());
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
    // Fresh clone has nothing balls-shaped in the working tree — XDG.
    assert!(!dev_b.path().join(".balls").exists());
    bl(dev_b.path()).arg("init").assert().success();
    // Per-clone state materialized under HOME (XDG), not in the tree.
    assert!(claims_dir(dev_b.path()).exists());
    assert!(!dev_b.path().join(".balls").exists());
}

#[test]
fn story_73_init_in_repo_with_no_commits() {
    // XDG `bl init` writes nothing on `main`, so it works against a
    // fresh `git init`'d clone with zero commits — provided origin is
    // set so the bootstrap branch has somewhere to push.
    let remote_dir = tmp();
    git(remote_dir.path(), &["init", "-q", "--bare", "-b", "main"]);
    let dir = tmp();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.email", "t@t"]);
    git(dir.path(), &["config", "user.name", "t"]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    git(
        dir.path(),
        &["remote", "add", "origin", &remote_dir.path().to_string_lossy()],
    );
    bl(dir.path()).arg("init").assert().success();
    // Clone has no balls footprint in the working tree.
    assert!(!dir.path().join(".balls").exists());
    // Tracker checkout exists at the documented XDG path.
    let bases = XdgBases::with(&test_home_path(), None, None, None);
    let url = remote_dir.path().to_string_lossy().into_owned();
    let enc = percent_encode_component(&canonicalize_origin(&url));
    let expected = bases.state_root().join("trackers").join(&enc).join(ENC_BALLS_TASKS);
    assert!(expected.join(".git").exists());
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
    let remote = tmp();
    let dir = tmp();

    // Build an empty bare remote so XDG bl init has an origin to point at.
    let mut grem = StdCommand::new("git");
    grem.current_dir(remote.path())
        .args(["init", "-q", "--bare", "-b", "main"])
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for var in GIT_ENV_VARS {
        grem.env_remove(var);
    }
    assert!(grem.output().expect("bare init").status.success());

    // Initialize a clone without configuring any user.email/user.name.
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

    // Wire origin so XDG init has a tracker address.
    let mut gr = StdCommand::new("git");
    gr.current_dir(dir.path())
        .args(["remote", "add", "origin", &remote.path().to_string_lossy()])
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for var in GIT_ENV_VARS {
        gr.env_remove(var);
    }
    assert!(gr.output().expect("git remote add").status.success());

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
    for key in ["user.email", "user.name"] {
        let mut clear = StdCommand::new("git");
        clear
            .current_dir(dir.path())
            .args(["config", "--local", "--unset", key])
            .env("HOME", home.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        for var in GIT_ENV_VARS {
            clear.env_remove(var);
        }
        let _ = clear.output();
    }

    bl_fresh().args(["create", "fresh task"]).assert().success();

    // `bl claim` branches `work/<id>` off the current `main`. XDG bl
    // init no longer seeds an empty commit on `main` (SPEC §14.19), so
    // the test seeds one itself before exercising the claim path —
    // mirrors what a real project (which has commits long before
    // anyone runs `bl init`) already has.
    let mut seed = StdCommand::new("git");
    seed.current_dir(dir.path())
        .args(["commit", "--allow-empty", "-qm", "seed", "--no-verify"])
        .env("HOME", home.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null");
    for var in GIT_ENV_VARS {
        seed.env_remove(var);
    }
    assert!(seed.output().expect("seed commit").status.success());

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
