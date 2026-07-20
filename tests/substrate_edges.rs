//! End-to-end substrate edges for the real `bl` lifecycle (bl-7b9f): the three
//! places the delivery/clock plumbing meets an unusual-but-legal substrate, each
//! driven through the freshly-built binary on a throwaway BARE project repo in a
//! tempdir with isolated HOME/XDG — never the dev repo's own store.
//!
//! 1. A BARE repo is balls' common deployment (`is_git_repo` accepts it, §11):
//!    close must deliver the tagged squash onto `main` exactly as a work-tree repo.
//! 2. `rm -rf`ing a materialized `work/<id>` dir leaves git's worktree
//!    registration STALE (`prunable`); a bare re-add would abort "missing but
//!    already registered" (bl-b404). The re-claim (unclaim → claim, since a task
//!    claimed-by-me refuses a second claim) must `worktree prune` then re-add —
//!    recover, not error.
//! 3. A `bl conf set clock-provider <bin>` value (the per-clone binding, bl-cfe3)
//!    emitting a fixed instant T dates the whole op from one read (§8, bl-8b98):
//!    frontmatter `created`/`updated` AND the delivered `main` commit's author +
//!    committer dates all equal T.
//!
//! tarpaulin counts src/ only, so this integration file is coverage-neutral.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

/// `bl` rooted in `project`, HOME/`XDG_STATE_HOME` pinned under the tempdir so
/// the store clone never touches the real `$HOME`; the shipped plugins resolve
/// beside the built `bl`. The inherited `BALLS_*` recursion bookkeeping is
/// scrubbed — this file itself runs inside a `bl close` gate under the orchestrator,
/// and a top-level `bl` here must start at depth 0.
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

/// `git -C <cwd> <args>` capturing trimmed stdout (a delivered subject / date).
fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed in {}", cwd.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A verb's one stdout product (create's id, claim's worktree path), trimmed.
fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// Parse `bl list --json`, tolerating the empty-list "" the read verbs emit when
/// nothing is live (§13) — a `[]` for `serde_json`.
fn live(json: &str) -> Vec<Value> {
    serde_json::from_str(if json.trim().is_empty() { "[]" } else { json }).unwrap()
}

/// A BARE project repo on `main` (balls' common deployment) plus a primed
/// checkout: seed a normal repo, `clone --bare` it, set the identity the delivery
/// `commit-tree` reads, and `bl prime` the stealth store under the tempdir.
fn bare_project(tmp: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let (home, state) = (tmp.join("h"), tmp.join("s"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&state).unwrap();

    let seed = tmp.join("seed");
    git(tmp, &["init", "-q", "-b", "main", &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "t"]);
    git(&seed, &["config", "user.email", "t@t"]);
    fs::write(seed.join("seed.txt"), "seed\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "seed"]);

    let project = tmp.join("proj.git");
    git(tmp, &["clone", "-q", "--bare", &seed.to_string_lossy(), &project.to_string_lossy()]);
    git(&project, &["config", "user.name", "t"]);
    git(&project, &["config", "user.email", "t@t"]);
    bl(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

#[test]
fn bare_repo_close_delivers_the_tagged_squash_exactly_as_a_worktree_repo() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = bare_project(tmp.path());

    let id = stdout(bl(&project, &home, &state).args(["create", "Bare work", "--as", "me"]).assert().success());
    let wt = stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "me"]).assert().success());

    // Work in the materialized code worktree hung off the bare root.
    fs::write(Path::new(&wt).join("feature.txt"), "shipped\n").unwrap();
    git(Path::new(&wt), &["add", "-A"]);
    git(Path::new(&wt), &["commit", "-qm", &format!("add feature [{id}]")]);

    bl(&project, &home, &state).args(["close", &id, "--as", "me"]).assert().success();

    // The squash landed on the bare repo's `main` — tagged subject + tree change,
    // identical to a work-tree repo's delivery.
    assert_eq!(git_out(&project, &["log", "-1", "--format=%s", "main"]), format!("Bare work [{id}]"));
    assert_eq!(git_out(&project, &["show", "main:feature.txt"]), "shipped");
    // Sealed + archived: the task is no longer live.
    let json = stdout(bl(&project, &home, &state).args(["list", "--json"]).assert().success());
    assert!(live(&json).iter().all(|t| t["id"] != id.as_str()), "close archived the task");
}

#[test]
fn a_stale_worktree_registration_is_pruned_on_reclaim_not_errored() {
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = bare_project(tmp.path());

    let id = stdout(bl(&project, &home, &state).args(["create", "Recover me", "--as", "me"]).assert().success());
    let wt = stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "me"]).assert().success());
    assert!(Path::new(&wt).join("seed.txt").exists(), "claim materialized the worktree");

    // Crash / tmp-cleaner / human: the dir vanishes but git keeps the
    // registration — now `prunable`, and a plain `worktree add` would abort.
    fs::remove_dir_all(&wt).unwrap();
    assert!(git_out(&project, &["worktree", "list"]).contains("prunable"), "registration is stale");

    // A second claim of a task claimed-by-me is refused — the sanctioned re-claim
    // is unclaim → claim. `unclaim`'s release no-ops on the missing dir (leaving
    // the stale registration), then `claim` prunes it and re-adds.
    bl(&project, &home, &state)
        .args(["claim", &id, "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("already claimed by me"));
    bl(&project, &home, &state).args(["unclaim", &id, "--as", "me"]).assert().success();

    let wt2 = stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "me"]).assert().success());
    assert_eq!(wt2, wt, "re-claim recomputes the same path");
    // Recovered, not errored: the worktree is back with content, and git no
    // longer holds a dangling `prunable` registration (the prune ran).
    assert!(Path::new(&wt).join("seed.txt").exists(), "re-claim re-materialized");
    assert!(!git_out(&project, &["worktree", "list"]).contains("prunable"), "stale registration pruned");
}

/// Write an executable fake clock provider at `path` that prints `t` (one
/// unix-seconds line) on every run — the `bl conf set clock-provider` seam.
fn fixed_clock(path: &Path, t: i64) {
    fs::write(path, format!("#!/bin/sh\necho {t}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn a_clock_provider_dates_the_frontmatter_and_the_delivery_squash() {
    const T: i64 = 1_700_000_000;
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = bare_project(tmp.path());

    // Point this clone's binding at a provider emitting the fixed instant T.
    let clock = tmp.path().join("clock.sh");
    fixed_clock(&clock, T);
    bl(&project, &home, &state).args(["conf", "set", "clock-provider", &clock.to_string_lossy()]).assert().success();

    // create/claim/close now each read T once — frontmatter `created`/`updated`
    // (the store SSOT, surfaced by `show --json`) are both T.
    let id = stdout(bl(&project, &home, &state).args(["create", "Clocked", "--as", "me"]).assert().success());
    let wt = stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "me"]).assert().success());
    let show = stdout(bl(&project, &home, &state).args(["show", &id, "--json"]).assert().success());
    let v: Value = serde_json::from_str(&show).unwrap();
    assert_eq!(v["created"], T, "frontmatter created = provider instant");
    assert_eq!(v["updated"], T, "frontmatter updated = provider instant");

    fs::write(Path::new(&wt).join("feature.txt"), "shipped\n").unwrap();
    git(Path::new(&wt), &["add", "-A"]);
    git(Path::new(&wt), &["commit", "-qm", &format!("work [{id}]")]);
    bl(&project, &home, &state).args(["close", &id, "--as", "me"]).assert().success();

    // The delivered `main` commit inherited the SAME instant for BOTH author (%at)
    // and committer (%ct) dates — the op-instant SSOT reaching the plugin squash.
    let dates = git_out(&project, &["log", "-1", "--format=%at%n%ct", "main"]);
    for line in dates.lines() {
        assert_eq!(line, T.to_string(), "delivery squash date = provider instant: {dates}");
    }
}
