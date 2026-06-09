//! End-to-end harness for the deliverable `update`/`claim`/`show` round-trips —
//! split from `tests/dispatch.rs` (the 300-line cap). Each test runs the
//! freshly-built `bl` against a throwaway temp project (a real git repo on
//! `main`, so the delivery plugin can fork `work/<id>` worktrees), never the
//! dev repo's own task list.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under `home`
/// and `state` so its clone bundle lands in the tempdir, not the real `$HOME`.
fn bl_primed(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// Run `git -C <cwd> <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A real project repo on `main` with a seed commit, plus a primed checkout.
fn primed_project(tmp: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    bl_primed(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

/// The id `bl create` printed alone to stdout (§9).
fn created_id(out: assert_cmd::assert::Assert) -> String {
    String::from_utf8(out.get_output().stdout.clone()).unwrap().trim().to_string()
}

#[test]
fn update_unlinks_a_blocker_so_a_wedged_claim_succeeds() {
    // §10 in-band recovery: a task blocked from claim by an unresolved edge is
    // freed by `bl update --no-needs`, no store-file surgery — the case that
    // keeps the no-cycle-detector deletion (bl-a38e) honest.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    // B can't be claimed until A resolves (default `--needs` op = claim, §10).
    let a = created_id(bl_primed(&project, &home, &state).args(["create", "Blocker A", "--as", "me"]).assert().success());
    let b = created_id(
        bl_primed(&project, &home, &state).args(["create", "Blocked B", "--needs", &a, "--as", "me"]).assert().success(),
    );

    // The wedge: claim refuses, naming the unresolved blocker.
    bl_primed(&project, &home, &state).args(["claim", &b, "--as", "me"]).assert().failure().stderr(contains(a.clone()));

    // Unlink the edge in-band — then the very same claim goes through, printing
    // the materialized worktree path as its stdout product (§11, bl-0af4).
    bl_primed(&project, &home, &state).args(["update", &b, "--no-needs", &a, "--as", "me"]).assert().success();
    let out = bl_primed(&project, &home, &state).args(["claim", &b, "--as", "me"]).assert().success();
    let path = String::from_utf8(out.get_output().stdout.clone()).unwrap().trim().to_string();
    assert!(path.ends_with(&b), "claim prints the worktree path, got: {path:?}");

    // `bl show` (human) folds the worktree line in via the §6 read dispatch;
    // `--json` never dispatches — the lossless store mirror carries no path.
    bl_primed(&project, &home, &state)
        .args(["show", &b])
        .assert()
        .success()
        .stdout(contains(format!("worktree {path}")));
    bl_primed(&project, &home, &state)
        .args(["show", &b, "--json"])
        .assert()
        .success()
        .stdout(contains("worktree").not());
}

#[test]
fn update_overwrites_every_field_end_to_end() {
    // The create-only split is gone: title, body, and parent are all editable
    // after the fact through the real CLI → engine → store round-trip (bl-9703).
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    // `--body` sets the ball's markdown body at create (not a commit note).
    let p = created_id(bl_primed(&project, &home, &state).args(["create", "Parent", "--as", "me"]).assert().success());
    let id = created_id(
        bl_primed(&project, &home, &state)
            .args(["create", "Old title", "--body", "first draft", "--as", "me"])
            .assert()
            .success(),
    );
    bl_primed(&project, &home, &state).args(["show", &id]).assert().success().stdout(contains("first draft"));

    // Retitle, rewrite the body, and reparent — all in one update.
    bl_primed(&project, &home, &state)
        .args(["update", &id, "--title", "New title", "--body", "rewritten", "--parent", &p, "--as", "me"])
        .assert()
        .success();
    bl_primed(&project, &home, &state)
        .args(["show", &id, "--json"])
        .assert()
        .success()
        .stdout(contains("\"New title\"").and(contains(format!("\"{p}\""))));
    bl_primed(&project, &home, &state).args(["show", &id]).assert().success().stdout(contains("rewritten"));

    // `--no-parent` clears the pointer back to null (bedrock always emits the key).
    bl_primed(&project, &home, &state).args(["update", &id, "--no-parent", "--as", "me"]).assert().success();
    bl_primed(&project, &home, &state)
        .args(["show", &id, "--json"])
        .assert()
        .success()
        .stdout(contains("\"parent\": null"));
}
