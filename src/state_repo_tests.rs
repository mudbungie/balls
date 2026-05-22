//! Unit tests for `state_repo`. Materialization tests stand up a
//! one-shot hub via a bare local repo so the codepath exercises the
//! online branch (`origin/balls/tasks` exists and gets tracked) and
//! the offline fallback (unreachable URL → safe-but-unlinked) without
//! a network dependency.

use super::test_support::hub_repo;
use super::*;
use crate::git;
use tempfile::TempDir;

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
fn ensure_creates_root_tasks_symlink_in_master_url_mode() {
    // bl-38dd: the legacy `setup_state_branch` leg drops a
    // `.balls/tasks -> worktree/.balls/tasks` symlink so engineers on
    // main can `ls .balls/tasks/` without any balls-specific knowledge.
    // The master_url leg bypasses that helper, so the symlink has to be
    // recreated here with the target swapped to the state-repo layout.
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();

    ensure(root.path(), &url).unwrap();

    let link = root.path().join(".balls/tasks");
    assert!(link.is_symlink(), "ensure must materialize the convenience symlink");
    let target = std::fs::read_link(&link).unwrap();
    assert_eq!(
        target,
        Path::new("state-repo/.balls/tasks"),
        "symlink target must reach the state-repo's tasks dir"
    );
    // Resolving through the symlink must hit the seeded tasks directory.
    assert!(link.join(".gitkeep").exists());
}

#[test]
fn ensure_repoints_stale_legacy_symlink_after_remaster() {
    // bl-773e: a repo initialized in legacy mode lands a
    // `.balls/tasks -> worktree/.balls/tasks` symlink. `bl remaster
    // --commit <url>` flips it to master_url mode and `state_repo::ensure`
    // must repoint the symlink at `state-repo/.balls/tasks`; otherwise
    // the link silently dangles once `.balls/worktree/` is gone and the
    // README-advertised `ls .balls/tasks/` ergonomic breaks.
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls")).unwrap();
    std::os::unix::fs::symlink(
        PathBuf::from("worktree/.balls/tasks"),
        root.path().join(".balls/tasks"),
    )
    .unwrap();

    ensure(root.path(), &url).unwrap();

    let target = fs::read_link(root.path().join(".balls/tasks")).unwrap();
    assert_eq!(
        target,
        Path::new("state-repo/.balls/tasks"),
        "stale legacy symlink must be repointed at the state-repo layout"
    );
}

#[test]
fn ensure_refuses_pre_existing_non_symlink_at_tasks_path() {
    // The bl-init guard against clobbering a stray `.balls/tasks` dir
    // applies in master_url mode too — silently adopting it could
    // shadow uncommitted task files an operator put there by hand.
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls/tasks")).unwrap();

    let err = ensure(root.path(), &url).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unexpected non-symlink"), "guard message must surface: {msg}");
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
fn ensure_first_time_unreachable_hub_hard_fails(
) {
    // bl-dcd3: a cross-repo client with `master_url` set is a pure
    // pointer to the hub. If first-time setup can't reach it, the
    // only safe outcome is to stop — silently dropping to a local
    // orphan would let the user accumulate task changes that diverge
    // from the team. The error names the URL, the underlying fetch
    // failure, and the three resolution paths.
    let root = TempDir::new().unwrap();
    let url = "/this/path/does/not/exist/hub.git";

    let err = ensure(root.path(), url).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains(url), "error must name the URL: {msg}");
    assert!(
        msg.contains("could not reach state hub"),
        "error must call out the unreachable hub: {msg}"
    );
    assert!(
        msg.contains("remaster --detach"),
        "error must mention the standalone escape hatch: {msg}"
    );
    assert!(
        msg.contains("master_url"),
        "error must name the config field to edit: {msg}"
    );
    // Scaffold rolled back so the next attempt is a clean first-time,
    // not a half-built cache that would soft-fail.
    assert!(
        !root.path().join(".balls/state-repo").exists(),
        "first-time failure must leave no partial state-repo behind"
    );
}

#[test]
fn ensure_warm_cache_offline_soft_fails() {
    // Once a state-repo has been successfully materialized, a later
    // offline invocation continues from the local cache — parity with
    // normal git, and the only soft-fail path the cross-repo model
    // accepts. bl-dcd3.
    let hub = hub_repo();
    let url = hub.path().join("hub.git").to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();

    // Warm the cache against a reachable hub.
    let dir = ensure(root.path(), &url).unwrap();
    let sha = git::git_resolve_sha(&dir, "balls/tasks").unwrap();

    // Drop the hub: subsequent invocations must keep working from the
    // local cache rather than re-erroring.
    drop(hub);
    let dir2 = ensure(root.path(), &url).unwrap();
    assert_eq!(dir, dir2);
    let sha2 = git::git_resolve_sha(&dir2, "balls/tasks").unwrap();
    assert_eq!(sha, sha2, "warm cache offline must not lose history");
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
fn looks_like_url_rejects_bare_remote_names() {
    assert!(!looks_like_url("origin"));
    assert!(!looks_like_url("hub"));
    assert!(!looks_like_url("upstream"));
    // host:port style is a name we won't second-guess.
    assert!(!looks_like_url("anything:1234"));
}

