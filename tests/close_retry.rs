//! bl-893d: three `bl close` stories driven end-to-end through the freshly-built
//! binary on a throwaway NON-bare project repo (isolated HOME/XDG — never the dev
//! repo's own store). Delivery moves `main` by plumbing and the root checkout
//! goes stale, so every landed-content assertion reads git objects
//! (`git show main:<path>`), never the root working tree.
//!
//! 1. A failing project `pre-commit` gate aborts the real `close`: the task stays
//!    claimed and the worktree is up; fixing the hook and re-closing lands exactly
//!    one `[bl-id]` squash and archives the task. (The gate is proven at the
//!    plugin seam in `tests/delivery/gates.rs`; here it is the whole binary —
//!    core's seal/rollback wiring included.)
//! 2. The SKILL.md flagship footgun: an unrelated edit committed DIRECTLY on the
//!    project's `main` (root checkout, not the `work/<id>` worktree) closes clean
//!    and stays behind — it survives on `main`'s history but is never folded into
//!    the task's delivery squash, nor corrupted/lost.
//! 3. Closing a parent with open, non-gating children prints the informational
//!    notice, still lands the parent's squash, and leaves the children alive with
//!    dangling (display-only) parent pointers.
//!
//! tarpaulin counts src/ only, so this integration file is coverage-neutral.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

/// `bl` rooted in `project`, HOME/`XDG_STATE_HOME` pinned under the tempdir so the
/// store clone never touches the real `$HOME`; the shipped plugins resolve beside
/// the built `bl`. The inherited `BALLS_*` recursion bookkeeping is scrubbed —
/// this file itself runs inside a `bl close` gate under the orchestrator, so a
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

/// Install `script` as the project repo's executable `pre-commit` hook (the gate
/// `close` restores over the plumbing squash). Written whole then chmod'd — no
/// write-then-exec of a half-flushed file (the ETXTBSY flake class).
fn install_hook(root: &Path, script: &str) {
    let hook = root.join(".git/hooks/pre-commit");
    fs::write(&hook, script).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A NON-bare project repo seeded on `main`, plus a primed stealth store under the
/// tempdir. A working root checkout is what stories 1–2 need: story 1 hangs the
/// `pre-commit` hook off its `.git`, story 2 commits a stray edit in it.
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

/// `bl claim <id>` → the materialized `work/<id>` worktree path (claim's stdout).
fn claim(root: &Path, home: &Path, state: &Path, id: &str) -> PathBuf {
    PathBuf::from(stdout(bl(root, home, state).args(["claim", id, "--as", "me"]).assert().success()))
}

/// Commit `content` into `file` inside the work worktree — the task's real work.
fn work(wt: &Path, file: &str, content: &str, id: &str) {
    fs::write(wt.join(file), content).unwrap();
    git(wt, &["add", "-A"]);
    git(wt, &["commit", "-qm", &format!("work [{id}]")]);
}

#[test]
fn a_failing_pre_commit_gate_aborts_the_close_then_a_fix_delivers_one_squash() {
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let id = stdout(bl(&root, &home, &state).args(["create", "Add feature", "--as", "me"]).assert().success());
    let wt = claim(&root, &home, &state, &id);
    work(&wt, "feature.txt", "shipped\n", &id);

    // A failing hook aborts the close BEFORE core seals: main never moves, the
    // task stays claimed, the worktree stays up for the fix.
    install_hook(&root, "#!/bin/sh\nexit 1\n");
    bl(&root, &home, &state)
        .args(["close", &id, "--as", "me"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("delivery gate").and(contains("failed")));
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), "seed", "no squash on a failed gate");
    bl(&root, &home, &state).args(["show", &id]).assert().success().stdout(contains("status   claimed"));
    assert!(wt.join("feature.txt").exists(), "the worktree stays up for the fix");

    // Fix the hook (and prove it ran in the WORKTREE by requiring the work's own
    // file in $PWD); the retry seals — exactly one tagged squash lands, task gone.
    install_hook(&root, "#!/bin/sh\ntest -f feature.txt\n");
    bl(&root, &home, &state).args(["close", &id, "--as", "me"]).assert().success();
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), format!("Add feature [{id}]"));
    let subjects = git_out(&root, &["log", "--format=%s", "main"]);
    assert_eq!(subjects.matches(&format!("[{id}]")).count(), 1, "exactly one delivery squash: {subjects}");
    assert_eq!(git_out(&root, &["show", "main:feature.txt"]), "shipped", "the work landed");
    bl(&root, &home, &state).args(["show", &id]).assert().success().stdout(contains("status   closed"));
}

#[test]
fn an_edit_committed_straight_on_main_stays_behind_and_is_never_folded_in() {
    // The SKILL.md flagship footgun. `close` folds `main` into the work branch
    // before squashing, so a stray commit on `main` becomes the squash's PARENT —
    // its content is reachable from `main` but its diff never rides the delivery.
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let id = stdout(bl(&root, &home, &state).args(["create", "Add feature", "--as", "me"]).assert().success());
    let wt = claim(&root, &home, &state, &id);
    work(&wt, "feature.txt", "feature\n", &id);

    // The user commits an unrelated edit DIRECTLY on main in the root checkout —
    // NOT in the work/<id> worktree.
    fs::write(root.join("stray.txt"), "stray change\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "unrelated stray edit on main"]);

    bl(&root, &home, &state).args(["close", &id, "--as", "me"]).assert().success();

    // Closes clean: the tagged squash lands and delivers ONLY the feature.
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), format!("Add feature [{id}]"));
    assert_eq!(git_out(&root, &["show", "--name-only", "--format=", "main"]), "feature.txt", "the squash is feature-only");
    assert_eq!(git_out(&root, &["show", "main:feature.txt"]), "feature", "the work landed");
    // Left behind, not corrupted or lost: the stray edit survives verbatim on
    // main's history (it is the squash's parent), never folded into the delivery.
    assert_eq!(git_out(&root, &["show", "main:stray.txt"]), "stray change", "the stray edit stays on main untouched");
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main~"]), "unrelated stray edit on main", "still its own commit");
}

#[test]
fn closing_a_parent_with_open_children_notices_but_delivers_and_leaves_them_alive() {
    let tmp = TempDir::new().unwrap();
    let (root, home, state) = project(tmp.path());
    let pid = stdout(bl(&root, &home, &state).args(["create", "Parent epic", "--as", "me"]).assert().success());
    let wt = claim(&root, &home, &state, &pid);
    work(&wt, "parent.txt", "code\n", &pid);

    // Two children with a bare --parent — a display-only edge, NOT `--blocks
    // close`, so nothing gates the parent's retirement.
    let c1 = stdout(bl(&root, &home, &state).args(["create", "child one", "--parent", &pid, "--as", "me"]).assert().success());
    let c2 = stdout(bl(&root, &home, &state).args(["create", "child two", "--parent", &pid, "--as", "me"]).assert().success());

    // The close succeeds with an informational notice, never a block.
    bl(&root, &home, &state)
        .args(["close", &pid, "--as", "me"])
        .assert()
        .success()
        .stderr(contains("closed with 2 open children, none gating"));

    // The parent's squash still lands on main...
    assert_eq!(git_out(&root, &["log", "-1", "--format=%s", "main"]), format!("Parent epic [{pid}]"));
    assert_eq!(git_out(&root, &["show", "main:parent.txt"]), "code", "the parent's work landed");
    // ...and both children survive, still rendering the now-dangling parent id.
    for c in [&c1, &c2] {
        bl(&root, &home, &state).args(["show", c]).assert().success().stdout(contains(pid.as_str()).and(contains("parent")));
    }
}
