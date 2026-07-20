//! End-to-end lock on the occupancy + identity guard surface of the lifecycle
//! verbs, driving the freshly-built `bl` against throwaway temp projects (never
//! the dev repo's own task list).
//!
//! The design has exactly ONE claimant-keyed refusal: OCCUPANCY. A claim on an
//! already-claimed ball is rejected (`change.rs`: "already claimed by <who>").
//! There is deliberately NO identity guard on `unclaim`/`close`: `--as` is only
//! the actor that rides the §5 commit, not a lock — `unclaim` clears the
//! claimant for ANYONE (it is the orphan-claim takeover primitive: no identity
//! check, no `--force`, no `drop`), and `close` retires regardless of who holds
//! it. These tests pin that real behavior: the occupancy refusal fires and never
//! mutates the store, while a mismatched-`--as` unclaim/close is HONORED, not
//! refused.

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under `home`
/// and `state` so its clone bundle lands in the tempdir, not the real `$HOME`;
/// `$XDG_CONFIG_HOME` removed, and any inherited plugin-chain env scrubbed so a
/// test run from inside the close-hook chain can't leak depth/name.
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

/// Run `git -C <cwd> <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A verb's one stdout product (create's id, claim's worktree path), trimmed.
fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// A real project repo on `main` with a seed commit, plus a primed stealth
/// landing — the delivery plugin can fork `work/<id>` worktrees off it.
fn primed_project(tmp: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    bl(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

/// The stored `claimant` field, read back through the lossless `show --json`
/// mirror (§3) — `Value::Null` when unclaimed, a `Value::String` otherwise.
fn claimant(project: &Path, home: &Path, state: &Path, id: &str) -> serde_json::Value {
    let out = bl(project, home, state).args(["show", id, "--json"]).assert().success();
    let json: serde_json::Value = serde_json::from_str(&stdout(out)).unwrap();
    json["claimant"].clone()
}

#[test]
fn a_double_claim_is_refused_and_the_claimant_is_left_untouched() {
    // The occupancy guard: once alice holds the ball, bob's claim is refused
    // nonzero, naming the incumbent — and the refusal aborts BEFORE any store
    // write, so the recorded claimant is still exactly `alice`.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    let id = stdout(bl(&project, &home, &state).args(["create", "Contended", "--as", "alice"]).assert().success());
    bl(&project, &home, &state).args(["claim", &id, "--as", "alice"]).assert().success();
    assert_eq!(claimant(&project, &home, &state, &id), "alice");

    // Second agent bob tries to claim the occupied ball → nonzero, and stderr
    // names WHO holds it ("already claimed by alice"), not a bare denial.
    bl(&project, &home, &state)
        .args(["claim", &id, "--as", "bob"])
        .assert()
        .failure()
        .stderr(contains(format!("{id} is already claimed by alice")));

    // The store is unchanged by the refusal — alice, not bob, still keys it.
    assert_eq!(claimant(&project, &home, &state, &id), "alice");
}

#[test]
fn a_non_claimant_unclaim_is_honored_not_refused() {
    // There is NO identity guard on unclaim: it is the takeover primitive. A
    // second agent (bob) who never held the ball can still release alice's
    // claim — the op succeeds and clears the claimant to null, no `--force`,
    // no identity check, no refusal. This pins the designed absence of a guard.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    let id = stdout(bl(&project, &home, &state).args(["create", "Held", "--as", "alice"]).assert().success());
    bl(&project, &home, &state).args(["claim", &id, "--as", "alice"]).assert().success();
    assert_eq!(claimant(&project, &home, &state, &id), "alice");

    // Mismatched `--as bob` — HONORED, not refused (exit 0), claimant cleared.
    bl(&project, &home, &state).args(["unclaim", &id, "--as", "bob"]).assert().success();
    assert_eq!(claimant(&project, &home, &state, &id), serde_json::Value::Null);
}

#[test]
fn a_second_agent_reclaims_after_a_legit_unclaim_then_a_mismatched_close_retires_it() {
    // The clean hand-off path plus the close identity non-guard:
    //   alice claims → alice unclaims (claimant null) → bob (a fresh agent) may
    //   now claim, re-keying the claimant to bob → carol, who never held it,
    //   closes it anyway (no identity guard on close), retiring the ball.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed_project(tmp.path());

    let id = stdout(bl(&project, &home, &state).args(["create", "Handoff", "--as", "alice"]).assert().success());
    bl(&project, &home, &state).args(["claim", &id, "--as", "alice"]).assert().success();

    // Legitimate release by the incumbent frees the ball for the next agent.
    bl(&project, &home, &state).args(["unclaim", &id, "--as", "alice"]).assert().success();
    assert_eq!(claimant(&project, &home, &state, &id), serde_json::Value::Null);

    // A second agent claims the now-free ball — the store re-keys to bob.
    bl(&project, &home, &state).args(["claim", &id, "--as", "bob"]).assert().success();
    assert_eq!(claimant(&project, &home, &state, &id), "bob");

    // close carries NO identity guard: carol (not the claimant) retires it.
    bl(&project, &home, &state).args(["close", &id, "--as", "carol"]).assert().success();

    // The ball is gone from the live list — closed, not merely unclaimed.
    let live = stdout(bl(&project, &home, &state).args(["list", "--json"]).assert().success());
    let live: serde_json::Value = serde_json::from_str(if live.trim().is_empty() { "[]" } else { &live }).unwrap();
    assert!(live.as_array().unwrap().iter().all(|t| t["id"] != id.as_str()), "close retired the ball: {live}");
}
