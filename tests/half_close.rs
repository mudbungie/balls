//! End-to-end DIRECTION lock for the half-close (bl-547f, bl-c3c0 Fix 3 (3)).
//!
//! close.pre squashes the delivery onto local `main` IRREVERSIBLY; the seal
//! archives the task; then `close.post = [bl-delivery, bl-tracker]` tears the
//! worktree DOWN (delivery) and only THEN pushes the store (tracker). When the
//! store push is rejected — a sibling moved the shared remote ahead — the op
//! aborts and core un-seals, re-opening the task. Teardown-before-push makes the
//! outcome DELIVERED + OPEN (squash on main, task re-opened, worktree gone),
//! never DONE + LEFTOVER. A future `close.post` reorder (push before teardown)
//! would silently flip this; this test, driving the real `bl` + both shipped
//! plugins against a shared remote, catches it.

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the
/// tempdir so its clone bundle never touches the real `$HOME`; the shipped
/// plugins resolve beside the built `bl`.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project).env("HOME", home).env("XDG_STATE_HOME", state).env_remove("XDG_CONFIG_HOME");
    cmd
}

/// `git -C <cwd> <args>`, asserting success (harness setup with plain git).
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// `git -C <cwd> <args>` capturing trimmed stdout (a delivery subject / blob).
fn git_out(cwd: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(cwd).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} failed in {}", cwd.display());
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A verb's one stdout product (create's id, claim's worktree path), trimmed.
fn stdout(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// Parse `bl list --json`, tolerating the empty-list "" the read verbs emit
/// when nothing is live (§13) — a `[]` for `serde_json`.
fn live(json: &str) -> serde_json::Value {
    serde_json::from_str(if json.trim().is_empty() { "[]" } else { json }).unwrap()
}

#[test]
fn a_rejected_close_post_push_leaves_delivered_plus_open_never_done_plus_leftover() {
    let tmp = TempDir::new().unwrap();
    let (home, state) = (tmp.path().join("h"), tmp.path().join("s"));

    // A bare origin seeded with `main` — the shared project repo + store host.
    let origin = tmp.path().join("origin.git");
    git(tmp.path(), &["init", "--bare", "-q", "-b", "main", &origin.to_string_lossy()]);
    let seed = tmp.path().join("seed");
    git(tmp.path(), &["clone", "-q", &origin.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["config", "user.name", "s"]);
    git(&seed, &["config", "user.email", "s@s"]);
    std::fs::write(seed.join("seed.txt"), "seed\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "seed"]);
    git(&seed, &["push", "-q", "origin", "main"]);

    // Alice's clone: prime founds balls/tasks on origin; identity for the
    // delivery squash (commit-tree reads the project repo's git config).
    let project = tmp.path().join("p");
    git(tmp.path(), &["clone", "-q", &origin.to_string_lossy(), &project.to_string_lossy()]);
    git(&project, &["config", "user.name", "alice"]);
    git(&project, &["config", "user.email", "alice@a"]);
    bl(&project, &home, &state).arg("prime").assert().success();

    let tid = stdout(bl(&project, &home, &state).args(["create", "Pave me", "--as", "alice"]).assert().success());
    let worktree = stdout(bl(&project, &home, &state).args(["claim", &tid, "--as", "alice"]).assert().success());
    std::fs::write(Path::new(&worktree).join("feature.txt"), "shipped\n").unwrap();
    git(Path::new(&worktree), &["add", "-A"]);
    git(Path::new(&worktree), &["commit", "-qm", &format!("add feature [{tid}]")]);

    // A sibling lands a commit on the shared store, so Alice is now LAGGING:
    // her optimistic close.post push will be rejected non-ff.
    let scratch = tmp.path().join("scratch");
    git(tmp.path(), &["clone", "-q", &origin.to_string_lossy(), &scratch.to_string_lossy()]);
    git(&scratch, &["config", "user.name", "bob"]);
    git(&scratch, &["config", "user.email", "bob@b"]);
    git(&scratch, &["checkout", "-q", "balls/tasks"]);
    std::fs::write(scratch.join("contention.txt"), "x\n").unwrap();
    git(&scratch, &["add", "-A"]);
    git(&scratch, &["commit", "-qm", "another writer"]);
    git(&scratch, &["push", "-q", "origin", "balls/tasks"]);

    // THE HALF-CLOSE: the squash lands locally, then the store push is rejected
    // — and the sharpened message (Fix 3 (2)) names the `bl sync` + retry recovery.
    bl(&project, &home, &state)
        .args(["close", &tid, "--as", "alice"])
        .assert()
        .failure()
        .stderr(contains("push rejected: the remote store moved ahead").and(contains("run `bl sync`, then re-run the command")));

    // DELIVERED: the irreversible squash stands on local main, carrying the tag.
    assert!(git_out(&project, &["log", "-1", "--format=%s", "main"]).contains(&format!("[{tid}]")));
    assert_eq!(git_out(&project, &["show", "main:feature.txt"]), "shipped");
    // OPEN: the abort un-sealed the archival — the task is live again, still claimed.
    let json = stdout(bl(&project, &home, &state).args(["list", "--json"]).assert().success());
    let reopened = live(&json);
    let t = reopened.as_array().unwrap().iter().find(|t| t["id"] == tid.as_str()).expect("T re-opened, not archived");
    assert_eq!(t["claimant"], "alice");
    // NEVER LEFTOVER: teardown ran (close.post bl-delivery) BEFORE the rejected
    // push (close.post bl-tracker) — the worktree is gone, not dangling.
    assert!(!Path::new(&worktree).exists(), "teardown-before-push: no done+leftover");

    // PAVED: `bl sync` + retry close converges — the squash already stands, so
    // deliver-from-missing-worktree (Fix 3 (1)) is a clean no-op and the retry
    // archives the task without minting a duplicate delivery.
    bl(&project, &home, &state).arg("sync").assert().success();
    bl(&project, &home, &state).args(["close", &tid, "--as", "alice"]).assert().success();
    let json = stdout(bl(&project, &home, &state).args(["list", "--json"]).assert().success());
    assert!(live(&json).as_array().unwrap().iter().all(|t| t["id"] != tid.as_str()), "retry close archived T");
    let subjects = git_out(&project, &["log", "--format=%s", "main"]);
    let deliveries = subjects.lines().filter(|l| l.contains(&format!("[{tid}]"))).count();
    assert_eq!(deliveries, 1, "exactly one delivery, no duplicate squash on retry:\n{subjects}");
}
