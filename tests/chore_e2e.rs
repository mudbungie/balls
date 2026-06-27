//! End-to-end harness for `bl-chore` (epic bl-6ee9): wire the real plugin into a
//! throwaway repo (freshly-built binaries, isolated XDG, /tmp repo — the E2E
//! demo-artifact convention) and prove the whole lifecycle balls drives, which
//! the src/ unit tests (fake `bl`) cannot reach. tarpaulin counts src/ only, so
//! this file is coverage-neutral.
//!
//! Each test stands up its own isolated substrate (own HOME/XDG/repo), so they
//! run in parallel without sharing state. The plugin's binary is found beside
//! `bl` via a `config/plugins/bin/bl-chore` symlink we drop BEFORE wiring the
//! schedule (an unbound hooked name aborts the op); `bl-chore` shells `bl` on
//! `$PATH`, so we put the cargo bin dir on the child's PATH.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Sys;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

/// A wired, isolated substrate: the project repo plus the env every `bl` call
/// needs (own HOME/XDG, and a PATH carrying the freshly-built binaries).
struct Env {
    home: PathBuf,
    state: PathBuf,
    config: PathBuf,
    repo: PathBuf,
    path: String,
}

/// `git -C <repo> <args>`, asserting success — repo setup only.
fn git(repo: &Path, args: &[&str]) {
    assert!(Sys::new("git").arg("-C").arg(repo).args(args).status().unwrap().success());
}

/// Run `bl <args>` in the wired env; return the raw output. The inherited
/// `BALLS_*` recursion bookkeeping is scrubbed so a top-level `bl` here always
/// starts at depth 0 — this test runs INSIDE a `bl` invocation when the
/// pre-commit gate fires under `bl close`, and must not inherit its odometer.
fn bl(e: &Env, args: &[&str]) -> std::process::Output {
    Command::cargo_bin("bl")
        .unwrap()
        .args(args)
        .env("HOME", &e.home)
        .env("XDG_STATE_HOME", &e.state)
        .env("XDG_CONFIG_HOME", &e.config)
        .env("PATH", &e.path)
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME")
        .current_dir(&e.repo)
        .output()
        .unwrap()
}

/// Run `bl`, assert it succeeded, return trimmed stdout (a verb's one product).
fn bl_ok(e: &Env, args: &[&str]) -> String {
    let out = bl(e, args);
    assert!(out.status.success(), "bl {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `bl list <extra> --json` parsed to bedrock rows.
fn list(e: &Env, extra: &[&str]) -> Vec<Value> {
    let mut args = vec!["list"];
    args.extend_from_slice(extra);
    args.push("--json");
    serde_json::from_str(&bl_ok(e, &args)).unwrap()
}

/// The live rows tagged `bl-chore` whose parent is `parent` — the minted chores.
fn chores_of(e: &Env, parent: &str) -> Vec<Value> {
    list(e, &[])
        .into_iter()
        .filter(|v| v["parent"].as_str() == Some(parent))
        .filter(|v| v["tags"].as_array().is_some_and(|ts| ts.iter().any(|t| t == "bl-chore")))
        .collect()
}

/// Stand up an isolated, bl-chore-wired substrate with `chores_toml` as config.
fn setup(chores_toml: &str) -> (TempDir, Env) {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join("home");
    let repo = tmp.path().join("repo");
    for d in [&home, &repo] {
        fs::create_dir_all(d).unwrap();
    }
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "e@e"]);
    git(&repo, &["config", "user.name", "e"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);

    let bin_dir = Path::new(env!("CARGO_BIN_EXE_bl")).parent().unwrap();
    let e = Env {
        home,
        state: tmp.path().join("state"),
        config: tmp.path().join("config"),
        repo,
        path: format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap_or_default()),
    };
    bl_ok(&e, &["prime", "--as", "e2e", "--stealth"]);

    // Bind the plugin binary beside the others (the gitignored symlink balls
    // resolves a hooked name to), THEN wire the schedule — order matters: an
    // unbound name in claim.post would abort the claim.
    let landing = fs::read_dir(e.state.join("balls/clones")).unwrap().next().unwrap().unwrap().path().join("config");
    let bin = landing.join("config/plugins/bin");
    fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_bl-chore"), bin.join("bl-chore")).unwrap();
    bl_ok(&e, &["conf", "prepend", "claim.post", "bl-chore"]);

    let cfg = landing.join("config/plugins/bl-chore");
    fs::create_dir_all(&cfg).unwrap();
    fs::write(cfg.join("chores.toml"), chores_toml).unwrap();
    (tmp, e)
}

const TWO: &str = "[[chore]]\ntitle = \"Run the test suite\"\n[[chore]]\ntitle = \"Review the docs\"\n";

#[test]
fn claim_mints_a_tagged_close_gate_child_per_chore() {
    let (_tmp, e) = setup(TWO);
    let x = bl_ok(&e, &["create", "real work", "--as", "e2e"]);
    bl_ok(&e, &["claim", &x, "--as", "e2e"]);

    let chores = chores_of(&e, &x);
    assert_eq!(chores.len(), 2);
    let titles: Vec<&str> = chores.iter().map(|c| c["title"].as_str().unwrap()).collect();
    assert!(titles.contains(&"Run the test suite") && titles.contains(&"Review the docs"));

    // The parent is gated on close by exactly those two chores.
    let parent = list(&e, &[]).into_iter().find(|v| v["id"] == x.as_str()).unwrap();
    let gates: Vec<&str> = parent["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["on"] == "close")
        .map(|b| b["id"].as_str().unwrap())
        .collect();
    assert_eq!(gates.len(), 2);
    for c in &chores {
        assert!(gates.contains(&c["id"].as_str().unwrap()));
    }
}

#[test]
fn the_chores_are_ready_children_and_the_parent_stays_claimed() {
    // The corrected expectation (the "zero ready-list clutter" claim was wrong):
    // a close-gate does NOT make the parent blocked, and the gate CHILDREN are
    // ordinary READY tasks that show in `bl list`.
    let (_tmp, e) = setup(TWO);
    let x = bl_ok(&e, &["create", "real work", "--as", "e2e"]);
    bl_ok(&e, &["claim", &x, "--as", "e2e"]);

    let ready_ids: Vec<String> = list(&e, &["-s", "ready"]).iter().map(|v| v["id"].as_str().unwrap().to_string()).collect();
    for c in chores_of(&e, &x) {
        assert!(ready_ids.contains(&c["id"].as_str().unwrap().to_string()), "chore should be READY");
    }
    // The parent itself is claimed (a close-gate never makes it `blocked`).
    let claimed: Vec<String> = list(&e, &["-s", "claimed"]).iter().map(|v| v["id"].as_str().unwrap().to_string()).collect();
    assert!(claimed.contains(&x));
}

#[test]
fn close_is_refused_until_the_chores_close() {
    let (_tmp, e) = setup(TWO);
    let x = bl_ok(&e, &["create", "real work", "--as", "e2e"]);
    bl_ok(&e, &["claim", &x, "--as", "e2e"]);

    let out = bl(&e, &["close", &x, "--as", "e2e"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("blocked by"));
}

#[test]
fn claiming_a_chore_does_not_recurse() {
    let (_tmp, e) = setup(TWO);
    let x = bl_ok(&e, &["create", "real work", "--as", "e2e"]);
    bl_ok(&e, &["claim", &x, "--as", "e2e"]);
    let chore = chores_of(&e, &x)[0]["id"].as_str().unwrap().to_string();

    // Claiming a chore fires claim.post again; tag-skip must bail (a chore is a
    // leaf, so the epic-skip has-children check could not catch it).
    bl_ok(&e, &["claim", &chore, "--as", "e2e"]);
    assert!(chores_of(&e, &chore).is_empty(), "no chore-of-a-chore");
}

#[test]
fn epic_skip_is_idempotent_and_the_knob_flips_it() {
    let (_tmp, e) = setup(TWO);
    let x = bl_ok(&e, &["create", "real work", "--as", "e2e"]);
    bl_ok(&e, &["claim", &x, "--as", "e2e"]);
    assert_eq!(chores_of(&e, &x).len(), 2);

    // Default-on epic-skip: a re-claim finds the existing children and does not
    // duplicate (the worktree is gone after unclaim, but the chores persist).
    bl_ok(&e, &["unclaim", &x, "--as", "e2e"]);
    bl_ok(&e, &["claim", &x, "--as", "e2e"]);
    assert_eq!(chores_of(&e, &x).len(), 2, "epic-skip is idempotent");

    // Flip the knob off in the plugin's own config: now a re-claim mints again.
    let landing = fs::read_dir(e.state.join("balls/clones")).unwrap().next().unwrap().unwrap().path().join("config");
    fs::write(landing.join("config/plugins/bl-chore/chores.toml"), format!("epic_skip = false\n{TWO}")).unwrap();
    bl_ok(&e, &["unclaim", &x, "--as", "e2e"]);
    bl_ok(&e, &["claim", &x, "--as", "e2e"]);
    assert_eq!(chores_of(&e, &x).len(), 4, "knob off re-mints");
}
