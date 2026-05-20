//! Unit tests for `state_repo`. Materialization tests stand up a
//! one-shot hub via a bare local repo so the codepath exercises the
//! online branch (`origin/balls/tasks` exists and gets tracked) and
//! the offline fallback (unreachable URL → safe-but-unlinked) without
//! a network dependency.

use super::*;
use crate::git;
use std::process::Command;
use tempfile::TempDir;

fn run(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed: {out:?}");
}

/// Spin up a bare repo with `balls/tasks` seeded to act as a hub URL.
fn hub_repo() -> TempDir {
    // Build the seed in a working clone, then push to the bare hub —
    // `git init --bare` alone has no balls/tasks ref to fetch.
    let scratch = TempDir::new().unwrap();
    let work = scratch.path().join("work");
    let bare = scratch.path().join("hub.git");
    std::fs::create_dir_all(&work).unwrap();
    run(&work, &["init", "-q", "--initial-branch", "balls/tasks"]);
    run(&work, &["config", "user.email", "t@e.x"]);
    run(&work, &["config", "user.name", "t"]);
    std::fs::write(work.join("seed"), "seed\n").unwrap();
    run(&work, &["add", "seed"]);
    run(&work, &["commit", "-qm", "seed"]);
    run(scratch.path(), &["clone", "--bare", "-q", work.to_str().unwrap(), bare.to_str().unwrap()]);
    // Hand the caller back a fresh TempDir holding only the bare so
    // the working scratch dropping doesn't take the hub with it.
    let kept = TempDir::new().unwrap();
    let dest = kept.path().join("hub.git");
    run(scratch.path(), &["clone", "--bare", "-q", bare.to_str().unwrap(), dest.to_str().unwrap()]);
    kept
}

#[test]
fn ensure_clones_from_reachable_hub_and_tracks_balls_tasks() {
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();

    let dir = ensure(root.path(), &url).unwrap();
    assert!(dir.join(".git").exists());
    assert!(git_state::branch_exists(&dir, "balls/tasks"));
    assert!(git_state::has_remote_branch(&dir, "origin", "balls/tasks"));
    assert!(dir.join(".balls/tasks/.gitattributes").exists());
    assert!(dir.join(".balls/tasks/.gitkeep").exists());
    assert_eq!(
        git::git_current_branch(&dir).unwrap(),
        "balls/tasks",
        "state-repo must have balls/tasks checked out"
    );
}

#[test]
fn ensure_is_idempotent() {
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();

    let dir1 = ensure(root.path(), &url).unwrap();
    let sha1 = git::git_resolve_sha(&dir1, "balls/tasks").unwrap();
    let dir2 = ensure(root.path(), &url).unwrap();
    let sha2 = git::git_resolve_sha(&dir2, "balls/tasks").unwrap();
    assert_eq!(dir1, dir2);
    assert_eq!(sha1, sha2, "re-running ensure must not re-root the branch");
}

#[test]
fn ensure_with_unreachable_url_creates_safe_local_orphan() {
    let root = TempDir::new().unwrap();
    let url = "/this/path/does/not/exist/hub.git";

    let dir = ensure(root.path(), url).unwrap();
    assert!(dir.join(".git").exists());
    assert!(git_state::branch_exists(&dir, "balls/tasks"));
    // The remote URL was still recorded — a later `bl prime` once the
    // hub becomes reachable picks up where this left off.
    let url_out = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(url_out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&url_out.stdout).trim(),
        url,
        "origin URL must be recorded even when unreachable"
    );
}

#[test]
fn looks_like_url_recognizes_common_forms() {
    assert!(looks_like_url("https://example.com/x.git"));
    assert!(looks_like_url("ssh://git@host/path"));
    assert!(looks_like_url("git@github.com:org/repo.git"));
    assert!(looks_like_url("/abs/path/hub.git"));
    assert!(looks_like_url("./relative/hub"));
    assert!(looks_like_url("../sibling/hub"));
}

#[test]
fn run_at_propagates_non_zero_git_exit() {
    // Outside a git repo: `git rev-parse --show-toplevel` exits non-zero.
    // Hits run_at's exit-status error path without needing a corrupted
    // .git surface.
    let dir = TempDir::new().unwrap();
    let err = run_at(dir.path(), &["rev-parse", "--show-toplevel"]).unwrap_err();
    match err {
        BallError::Git(msg) => assert!(
            msg.contains("exited with") || msg.contains("rev-parse"),
            "expected exit-status diagnostic, got: {msg}"
        ),
        other => panic!("expected Git error, got {other:?}"),
    }
}

#[test]
fn looks_like_url_rejects_bare_remote_names() {
    assert!(!looks_like_url("origin"));
    assert!(!looks_like_url("hub"));
    assert!(!looks_like_url("upstream"));
    // host:port style is a name we won't second-guess.
    assert!(!looks_like_url("anything:1234"));
}
