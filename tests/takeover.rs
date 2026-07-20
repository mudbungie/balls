//! End-to-end lock on the three SANCTIONED orphan-claim recovery stories the
//! rest of the suite leaves undriven (bl-354e), each run through the freshly-built
//! `bl` plus the shipped plugins against a shared bare origin in a tempdir with
//! isolated HOME/XDG — never the dev repo's own store.
//!
//! 1. SAME-CLONE takeover: alice claims, commits WIP on `work/<id>`, goes dark.
//!    bob `unclaim`s (honored — no identity check, §guards) then `claim`s as
//!    himself. alice's committed branch survives the unclaim, bob's claim
//!    re-attaches the SAME worktree onto the SAME branch — so it ALREADY carries
//!    alice's WIP (no cherry-pick needed), and bob's close squashes it onto `main`.
//! 2. CROSS-CLONE takeover: from a SECOND independent clone (own HOME/XDG, shared
//!    store via origin) bob unclaims + claims the same id. His `work/<id>` is
//!    FRESH/EMPTY — the branch is machine-local git, never pushed, so `skill/
//!    unclaim.md`'s "committed work survives" is FALSE across machines. alice's
//!    clone-side branch + worktree stay untouched. This pins reality (FINDING).
//! 3. `rm -rf`ed worktree, STILL CLAIMED, `close`d DIRECTLY (no unclaim/reclaim):
//!    close.pre re-materializes the absent worktree (§11) and delivers the
//!    branch's committed content. substrate_edges covers the rm -rf only via the
//!    unclaim→claim path; this drives the direct-close path.
//!
//! tarpaulin counts src/ only, so this integration file is coverage-neutral.

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted in `project`, HOME/`XDG_STATE_HOME` pinned under the tempdir so the
/// store clone never touches the real `$HOME`; the shipped plugins resolve beside
/// the built `bl`. Inherited `BALLS_*` recursion bookkeeping is scrubbed — this
/// file itself runs inside a `bl close` gate, and a top-level `bl` here starts at
/// depth 0.
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

/// `git -C <cwd> <args>` capturing trimmed stdout (a delivered subject / blob).
fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed in {}", cwd.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A verb's one stdout product (create's id, claim's worktree path), trimmed.
fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// Parse `bl list --json`, tolerating the empty-list "" read verbs emit (§13).
fn live(json: &str) -> Vec<Value> {
    serde_json::from_str(if json.trim().is_empty() { "[]" } else { json }).unwrap()
}

/// The stored `claimant` (§3 lossless `show --json` mirror) — `Null` when free.
fn claimant(project: &Path, home: &Path, state: &Path, id: &str) -> Value {
    let out = bl(project, home, state).args(["show", id, "--json"]).assert().success();
    let json: Value = serde_json::from_str(&stdout(out)).unwrap();
    json["claimant"].clone()
}

/// A bare origin seeded with `main` — the shared project repo + store host.
fn origin_with_seed(tmp: &Path) -> PathBuf {
    let origin = tmp.join("origin.git");
    git(tmp, &["init", "--bare", "-q", "-b", "main", &origin.to_string_lossy()]);
    let seed = tmp.join("seed");
    git(tmp, &["clone", "-q", &origin.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "s"]);
    git(&seed, &["config", "user.email", "s@s"]);
    fs::write(seed.join("seed.txt"), "seed\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-qm", "seed"]);
    git(&seed, &["push", "-q", "origin", "main"]);
    origin
}

/// An INDEPENDENT clone of `origin` under `tag`, with its OWN HOME/state, primed
/// so it adopts (or founds) the shared `balls/tasks` store. Identity is what the
/// delivery `commit-tree` reads.
fn clone_peer(tmp: &Path, origin: &Path, tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let (home, state, project) = (tmp.join(format!("{tag}-h")), tmp.join(format!("{tag}-s")), tmp.join(tag));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&state).unwrap();
    git(tmp, &["clone", "-q", &origin.to_string_lossy(), &project.to_string_lossy()]);
    git(&project, &["config", "user.name", tag]);
    git(&project, &["config", "user.email", &format!("{tag}@t")]);
    bl(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

#[test]
fn same_clone_takeover_delivers_the_orphaned_wip_after_unclaim_reclaim() {
    let tmp = TempDir::new().unwrap();
    let origin = origin_with_seed(tmp.path());
    let (project, home, state) = clone_peer(tmp.path(), &origin, "a");

    // alice claims and commits WIP on the work branch, then goes dark.
    let id = stdout(bl(&project, &home, &state).args(["create", "Orphaned", "--as", "alice"]).assert().success());
    let wt = stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "alice"]).assert().success());
    fs::write(Path::new(&wt).join("feature.txt"), "alice-wip\n").unwrap();
    git(Path::new(&wt), &["add", "-A"]);
    git(Path::new(&wt), &["commit", "-qm", &format!("wip [{id}]")]);

    // bob takes over: unclaim is HONORED with no identity check (§guards), and
    // tears the worktree down — but the committed `work/<id>` branch SURVIVES.
    bl(&project, &home, &state).args(["unclaim", &id, "--as", "bob"]).assert().success();
    assert!(!Path::new(&wt).exists(), "unclaim tore the worktree down");
    assert!(!git_out(&project, &["branch", "--list", &format!("work/{id}")]).is_empty(), "committed branch survives");

    // bob's claim re-attaches the SAME worktree onto the SAME surviving branch —
    // so it ALREADY carries alice's WIP; no cherry-pick is needed.
    let wt2 = stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "bob"]).assert().success());
    assert_eq!(wt2, wt, "re-claim recomputes the same path");
    assert_eq!(claimant(&project, &home, &state, &id), "bob");
    assert_eq!(fs::read_to_string(Path::new(&wt2).join("feature.txt")).unwrap(), "alice-wip\n", "branch carries WIP");

    // bob closes: the surviving WIP lands in the delivered squash on `main`.
    bl(&project, &home, &state).args(["close", &id, "--as", "bob"]).assert().success();
    assert_eq!(git_out(&project, &["show", "main:feature.txt"]), "alice-wip");
    assert_eq!(git_out(&project, &["log", "-1", "--format=%s", "main"]), format!("Orphaned [{id}]"));
    let json = stdout(bl(&project, &home, &state).args(["list", "--json"]).assert().success());
    assert!(live(&json).iter().all(|t| t["id"] != id.as_str()), "close archived the ball");
}

#[test]
fn cross_clone_takeover_strands_the_wip_the_docs_promise_survives() {
    let tmp = TempDir::new().unwrap();
    let origin = origin_with_seed(tmp.path());

    // alice's clone claims and commits WIP — the store push publishes the claim,
    // but the `work/<id>` branch is machine-local git and is NEVER pushed.
    let (a_proj, a_home, a_state) = clone_peer(tmp.path(), &origin, "alice");
    let id = stdout(bl(&a_proj, &a_home, &a_state).args(["create", "Cross", "--as", "alice"]).assert().success());
    let wt_a = stdout(bl(&a_proj, &a_home, &a_state).args(["claim", &id, "--as", "alice"]).assert().success());
    fs::write(Path::new(&wt_a).join("feature.txt"), "alice-machine-local\n").unwrap();
    git(Path::new(&wt_a), &["add", "-A"]);
    git(Path::new(&wt_a), &["commit", "-qm", &format!("wip [{id}]")]);

    // A SECOND independent clone (own HOME/XDG) adopts the shared store on prime,
    // so bob sees alice's published claim.
    let (b_proj, b_home, b_state) = clone_peer(tmp.path(), &origin, "bob");
    assert_eq!(claimant(&b_proj, &b_home, &b_state, &id), "alice", "the claim crossed via the shared store");

    // bob unclaims (honored) and claims from his clone.
    bl(&b_proj, &b_home, &b_state).args(["unclaim", &id, "--as", "bob"]).assert().success();
    let wt_b = stdout(bl(&b_proj, &b_home, &b_state).args(["claim", &id, "--as", "bob"]).assert().success());
    assert_eq!(claimant(&b_proj, &b_home, &b_state, &id), "bob");

    // FINDING (docs-mismatch): `skill/unclaim.md` promises "Work you committed on
    // the `work/<id>` branch survives: a later `bl claim` + `bl close` delivers
    // it." That holds only SAME-clone. Across machines the branch never crossed —
    // bob's worktree is materialized FRESH off `main`, and alice's WIP is stranded.
    assert!(!Path::new(&wt_b).join("feature.txt").exists(), "cross-clone: alice's WIP did NOT survive");
    assert!(Path::new(&wt_b).join("seed.txt").exists(), "bob's worktree is fresh off main");

    // alice's clone-side branch + worktree stay untouched — the WIP is stranded,
    // not lost: it still lives, in-place, on her machine.
    assert_eq!(fs::read_to_string(Path::new(&wt_a).join("feature.txt")).unwrap(), "alice-machine-local\n");
    assert!(!git_out(&a_proj, &["branch", "--list", &format!("work/{id}")]).is_empty(), "alice's branch intact");
}

#[test]
fn direct_close_of_an_rm_rf_ed_worktree_rematerializes_and_delivers() {
    let tmp = TempDir::new().unwrap();
    let origin = origin_with_seed(tmp.path());
    let (project, home, state) = clone_peer(tmp.path(), &origin, "a");

    // Claim, commit WIP, then the worktree dir vanishes (crash / tmp-cleaner) —
    // the ball is STILL claimed; no unclaim/reclaim dance happens.
    let id = stdout(bl(&project, &home, &state).args(["create", "Vanished", "--as", "me"]).assert().success());
    let wt = stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "me"]).assert().success());
    fs::write(Path::new(&wt).join("feature.txt"), "committed-wip\n").unwrap();
    git(Path::new(&wt), &["add", "-A"]);
    git(Path::new(&wt), &["commit", "-qm", &format!("wip [{id}]")]);
    fs::remove_dir_all(&wt).unwrap();
    assert!(!Path::new(&wt).exists(), "worktree dir is gone");

    // close DIRECTLY — close.pre re-materializes the absent worktree (§11) and
    // delivers the branch's committed content in one move, no reclaim needed.
    bl(&project, &home, &state).args(["close", &id, "--as", "me"]).assert().success();
    assert_eq!(git_out(&project, &["show", "main:feature.txt"]), "committed-wip");
    assert_eq!(git_out(&project, &["log", "-1", "--format=%s", "main"]), format!("Vanished [{id}]"));
    let json = stdout(bl(&project, &home, &state).args(["list", "--json"]).assert().success());
    assert!(live(&json).iter().all(|t| t["id"] != id.as_str()), "direct close archived the ball");
}
