//! §11/§15 the FORGE submit/approve flow through the real `bl` binary (bl-f1a4):
//! an agent pushes `work/<id>`, a HUMAN or forge squash-merges it into `main`
//! OUTSIDE bl — so ancestry is broken but the content matches verbatim — and the
//! agent then runs `bl close` only to RETIRE the task. Close must see the content
//! already landed (`Standing::Settled` via `git merge-tree` content-containment,
//! src/delivery_standing.rs) and converge: mint NO second `[bl-id]` squash, and
//! still archive the task + tear the worktree down. A regression here either
//! double-delivers or false-aborts every forge-based team's close.
//!
//! This is the through-the-binary proof the src unit (delivery_standing_tests.rs)
//! and the fake-repo integration (tests/delivery/standing.rs) never gave: a REAL
//! `git merge --squash` on `main` driving the REAL close plugin chain.
//!
//! tarpaulin counts src/ only, so this integration file is coverage-neutral.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

/// `bl` rooted in `project`, HOME/`XDG_STATE_HOME` pinned under the tempdir so the
/// store clone never touches the real `$HOME`; the shipped plugins resolve beside
/// the built `bl`. The inherited `BALLS_*` recursion bookkeeping is scrubbed —
/// this file itself runs inside a `bl close` gate under the orchestrator, and a
/// top-level `bl` here must start at depth 0.
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

/// `git -C <cwd> <args>` capturing trimmed stdout.
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

/// How many `[<id>]`-tagged commits stand on `main` — the no-duplicate invariant.
fn marked_count(project: &Path, id: &str) -> usize {
    let n = git_out(project, &["rev-list", "--count", "--fixed-strings", &format!("--grep=[{id}]"), "main"]);
    n.parse().unwrap()
}

/// The lone `refs/heads/work/*` branch the claim minted (the key may carry a
/// `-<claimant>` suffix, §11 — discover it, don't assume `work/<id>`).
fn work_branch(project: &Path) -> String {
    git_out(project, &["for-each-ref", "--format=%(refname:short)", "refs/heads/work/"])
}

/// A BARE project repo on `main` (balls' common deployment) plus a primed store:
/// seed a normal repo, `clone --bare` it, set the identity the delivery
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

/// The forge/human squash-merge OUTSIDE bl: check `main` out in a throwaway
/// linked worktree, `git merge --squash <branch>`, commit with `[<id>]` in the
/// subject (as the forge would with the PR title), and drop the worktree. This
/// advances the bare repo's `refs/heads/main` with a commit that shares NO
/// ancestry with `<branch>`'s commits yet carries their content verbatim — the
/// exact ancestry-broken / content-matching state a PR squash-merge leaves.
fn forge_squash_merge(tmp: &Path, project: &Path, branch: &str, subject: &str) {
    let forge = tmp.join("forge");
    git(project, &["worktree", "add", "-q", &forge.to_string_lossy(), "main"]);
    git(&forge, &["merge", "--squash", branch]);
    git(&forge, &["commit", "-qm", subject]);
    git(project, &["worktree", "remove", "--force", &forge.to_string_lossy()]);
}

#[test]
fn a_forge_squash_merge_then_close_retires_without_a_second_delivery() {
    // The documented submit/approve flow (skill/close.md "Submit/approve flows",
    // §11 FORGE): the agent works on work/<id>, a forge squash-merges the PR into
    // main OUTSIDE bl, then the agent runs `bl close` only to RETIRE the task.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = bare_project(tmp.path());

    let id = stdout(bl(&project, &home, &state).args(["create", "Ship it", "--as", "me"]).assert().success());
    let wt = stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "me"]).assert().success());

    // The agent's work, committed onto work/<id> (as if pushed for review).
    fs::write(Path::new(&wt).join("feature.txt"), "shipped\n").unwrap();
    git(Path::new(&wt), &["add", "-A"]);
    git(Path::new(&wt), &["commit", "-qm", "implement the feature"]);
    let branch = work_branch(&project);

    // The forge squash-merges the PR into main — a NEW commit, no shared ancestry
    // with the wip commit, `[<id>]` in the subject (the PR title convention).
    forge_squash_merge(tmp.path(), &project, &branch, &format!("Ship it (#7) [{id}]"));
    let forge_tip = git_out(&project, &["rev-parse", "main"]);
    assert_eq!(marked_count(&project, &id), 1, "the forge landed exactly one delivery");

    // The agent runs `bl close` only to retire the task. Content-containment
    // (Standing::Settled) must recognize the delivery already landed and CONVERGE.
    bl(&project, &home, &state).args(["close", &id, "--as", "me"]).assert().success();

    // No second squash minted: still exactly ONE [<id>] commit, and main is the
    // forge's own commit unmoved — close added nothing on top.
    assert_eq!(marked_count(&project, &id), 1, "close double-delivered a second squash");
    assert_eq!(git_out(&project, &["rev-parse", "main"]), forge_tip, "close moved main past the forge commit");
    assert_eq!(git_out(&project, &["log", "-1", "--format=%s", "main"]), format!("Ship it (#7) [{id}]"));
    assert_eq!(git_out(&project, &["show", "main:feature.txt"]), "shipped");

    // The task is archived (retired) and the worktree torn down.
    let json = stdout(bl(&project, &home, &state).args(["list", "--json"]).assert().success());
    assert!(live(&json).iter().all(|t| t["id"] != id.as_str()), "close did not archive the task");
    let closed = stdout(bl(&project, &home, &state).args(["list", "-s", "closed", "--json"]).assert().success());
    assert!(live(&closed).iter().any(|t| t["id"] == id.as_str()), "the task is not listed closed");
    assert!(!Path::new(&wt).exists(), "close did not tear the worktree down");
}

#[test]
fn work_not_yet_on_main_still_delivers_normally() {
    // The near-miss guarding against a Settled FALSE-POSITIVE: an ordinary close
    // whose content is NOT already on main must mint its `[<id>]` squash as usual.
    // If containment were too eager it would strand this work with a silent skip.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = bare_project(tmp.path());

    let id = stdout(bl(&project, &home, &state).args(["create", "Real work", "--as", "me"]).assert().success());
    let wt = stdout(bl(&project, &home, &state).args(["claim", &id, "--as", "me"]).assert().success());
    fs::write(Path::new(&wt).join("feature.txt"), "shipped\n").unwrap();
    git(Path::new(&wt), &["add", "-A"]);
    git(Path::new(&wt), &["commit", "-qm", "implement the feature"]);

    // Nothing was pre-landed on main — a normal `bl close` delivers.
    assert_eq!(marked_count(&project, &id), 0, "nothing on main before close");
    bl(&project, &home, &state).args(["close", &id, "--as", "me"]).assert().success();

    // Exactly one delivery squash landed, tagged + carrying the content.
    assert_eq!(marked_count(&project, &id), 1, "normal close did not deliver");
    assert_eq!(git_out(&project, &["log", "-1", "--format=%s", "main"]), format!("Real work [{id}]"));
    assert_eq!(git_out(&project, &["show", "main:feature.txt"]), "shipped");
    let json = stdout(bl(&project, &home, &state).args(["list", "--json"]).assert().success());
    assert!(live(&json).iter().all(|t| t["id"] != id.as_str()), "close did not archive the task");
    assert!(!Path::new(&wt).exists(), "close did not tear the worktree down");
}
