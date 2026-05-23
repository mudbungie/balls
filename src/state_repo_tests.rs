//! Unit tests for the unified `state_repo` materializer. Trackers are
//! bare local repos so every branch — online clone, offline fallback,
//! hard-fail, and in-place legacy migration — runs without a network.

use super::test_support::{empty_hub, explicit, hub_repo, hub_url, implicit, implicit_url, legacy_project};
use super::*;
use crate::git;

#[test]
fn ensure_clones_from_reachable_tracker_and_tracks_state_branch() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    let dir = ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();

    assert!(dir.join(".git").exists());
    assert!(git_state::branch_exists(&dir, "balls/tasks"));
    assert!(git_state::has_remote_branch(&dir, "origin", "balls/tasks"));
    assert!(dir.join(".balls/tasks/.gitattributes").exists());
    assert!(dir.join(".balls/plugins/.gitkeep").exists());
    assert_eq!(git::git_current_branch(&dir).unwrap(), "balls/tasks");
}

#[test]
fn ensure_implicit_default_no_url_inits_a_local_orphan() {
    // A solo repo with no `origin` is offline-bootstrappable (§9).
    let root = tempfile::TempDir::new().unwrap();
    let dir = ensure(root.path(), &implicit()).unwrap();
    assert!(dir.join(".git").exists());
    assert!(git_state::branch_exists(&dir, "balls/tasks"));
    assert!(!git::git_has_remote(&dir, "origin"));
}

#[test]
fn ensure_explicit_unreachable_tracker_hard_fails() {
    let root = tempfile::TempDir::new().unwrap();
    let url = "/no/such/explicit/tracker.git";
    let err = ensure(root.path(), &explicit(url)).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains(url), "names the URL: {msg}");
    assert!(msg.contains("could not reach state tracker"), "{msg}");
    assert!(msg.contains("remaster --detach"), "offers the escape hatch: {msg}");
    assert!(msg.contains("state_url"), "names the config field: {msg}");
    assert!(
        !root.path().join(".balls/state-repo").exists(),
        "a hard-fail leaves no partial checkout"
    );
}

#[test]
fn ensure_implicit_unreachable_falls_back_to_a_local_orphan() {
    // A legacy `state_remote` / implicit origin that is unreachable
    // stays safe-but-unlinked rather than hard-failing.
    let root = tempfile::TempDir::new().unwrap();
    let dir = ensure(root.path(), &implicit_url("/no/such/tracker.git")).unwrap();
    assert!(git_state::branch_exists(&dir, "balls/tasks"));
}

#[test]
fn ensure_seeds_an_orphan_when_the_tracker_has_no_state_branch() {
    let hub = empty_hub();
    let root = tempfile::TempDir::new().unwrap();
    let dir = ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();
    assert!(git_state::branch_exists(&dir, "balls/tasks"));
    assert!(dir.join(".balls/tasks/.gitattributes").exists());
}

#[test]
fn ensure_migrates_a_legacy_worktree_in_place() {
    let project = legacy_project();
    let root = project.path();
    let dir = ensure(root, &implicit()).unwrap();

    // The legacy task survived into the new checkout.
    assert!(dir.join(".balls/tasks/bl-legacytask.json").exists());
    // The legacy `.balls/worktree` and the project's own branch are gone.
    assert!(!root.join(".balls/worktree").exists());
    assert!(!git_state::branch_exists(root, "balls/tasks"));
    // The convenience symlink points at the new checkout.
    let target = std::fs::read_link(root.join(".balls/tasks")).unwrap();
    assert_eq!(target, Path::new("state-repo/.balls/tasks"));
}

#[test]
fn ensure_is_idempotent_and_warm_path_skips_the_network() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    let dir1 = ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();
    let sha1 = git::git_resolve_sha(&dir1, "balls/tasks").unwrap();
    // Drop the tracker — a warm checkout must not need it again.
    drop(hub);
    let dir2 = ensure(root.path(), &implicit()).unwrap();
    assert_eq!(dir1, dir2);
    assert_eq!(sha1, git::git_resolve_sha(&dir2, "balls/tasks").unwrap());
}

#[test]
fn ensure_warm_repoints_origin_at_the_address_url() {
    let root = tempfile::TempDir::new().unwrap();
    ensure(root.path(), &implicit()).unwrap(); // no origin
    let hub = hub_repo();
    let url = hub_url(&hub);
    let dir = ensure(root.path(), &explicit(&url)).unwrap();
    assert_eq!(git_state::remote_url(&dir, "origin").as_deref(), Some(url.as_str()));
}

#[test]
fn ensure_materializes_the_tasks_and_plugins_symlinks() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();
    let balls = root.path().join(".balls");
    assert!(balls.join("tasks").is_symlink());
    assert!(balls.join("plugins").is_symlink());
    assert_eq!(
        std::fs::read_link(balls.join("tasks")).unwrap(),
        Path::new("state-repo/.balls/tasks")
    );
    assert!(balls.join("tasks/.gitkeep").exists(), "symlink resolves into the checkout");
}

#[test]
fn ensure_repoints_a_stale_legacy_tasks_symlink() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls")).unwrap();
    std::os::unix::fs::symlink(
        PathBuf::from("worktree/.balls/tasks"),
        root.path().join(".balls/tasks"),
    )
    .unwrap();
    ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();
    assert_eq!(
        fs::read_link(root.path().join(".balls/tasks")).unwrap(),
        Path::new("state-repo/.balls/tasks")
    );
}

#[test]
fn ensure_repoints_a_stale_plugins_symlink() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls")).unwrap();
    std::os::unix::fs::symlink(
        PathBuf::from("old-stale-target"),
        root.path().join(".balls/plugins"),
    )
    .unwrap();
    ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();
    assert_eq!(
        fs::read_link(root.path().join(".balls/plugins")).unwrap(),
        Path::new("state-repo/.balls/plugins")
    );
}

#[test]
fn ensure_repoints_a_stale_project_json_symlink() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls")).unwrap();
    std::os::unix::fs::symlink(
        PathBuf::from("stale-project-target"),
        root.path().join(".balls/project.json"),
    )
    .unwrap();
    ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();
    assert_eq!(
        fs::read_link(root.path().join(".balls/project.json")).unwrap(),
        Path::new("state-repo/.balls/project.json")
    );
}

#[test]
fn ensure_refuses_a_pre_existing_non_symlink_at_the_project_json_path() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls")).unwrap();
    fs::write(root.path().join(".balls/project.json"), "{}").unwrap();
    let err = ensure(root.path(), &explicit(&hub_url(&hub))).unwrap_err();
    assert!(err.to_string().contains("unexpected non-symlink"), "{err}");
}

#[test]
fn ensure_project_config_migrates_a_warm_pre_split_checkout() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    let dir = ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();
    let link = root.path().join(".balls/project.json");
    let tracked = dir.join(".balls/project.json");

    // Simulate a checkout from before the config split: drop the
    // project.json file and its workspace symlink, and commit that.
    fs::remove_file(&link).unwrap();
    fs::remove_file(&tracked).unwrap();
    git::git_add_all(&dir).unwrap();
    git::git_commit(&dir, "drop project.json").unwrap();

    ensure_project_config(root.path(), &dir).unwrap();
    assert!(link.is_symlink() && tracked.exists(), "migration restores both");

    // Idempotent: a second call short-circuits on the resolved symlink.
    ensure_project_config(root.path(), &dir).unwrap();

    // Symlink-only loss: the tracked file survives, so no new commit —
    // only the link is rebuilt.
    fs::remove_file(&link).unwrap();
    ensure_project_config(root.path(), &dir).unwrap();
    assert!(link.is_symlink() && tracked.exists());
}

#[test]
fn ensure_refuses_migration_with_uncommitted_legacy_worktree_state() {
    // The migration adopts only the committed tip of balls/tasks via
    // fetch+reset; uncommitted edits and untracked files in the legacy
    // .balls/worktree would be silently discarded. Refuse instead.
    let project = legacy_project();
    let root = project.path();
    let wt = root.join(".balls/worktree");
    fs::write(wt.join(".balls/tasks/bl-legacytask.json"), "{\"dirty\":true}\n").unwrap();
    fs::write(wt.join(".balls/tasks/bl-untracked.json"), "{}\n").unwrap();

    let err = ensure(root, &implicit()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("uncommitted"), "names the situation: {msg}");
    assert!(msg.contains("bl-legacytask.json"), "lists the tracked edit: {msg}");
    assert!(msg.contains("bl-untracked.json"), "lists the untracked file: {msg}");
    assert!(msg.contains("git add -A && git commit"), "shows the recovery command: {msg}");

    // Recoverable: legacy worktree intact, no partial state-repo materialized.
    assert!(wt.exists());
    assert!(!root.join(".balls/state-repo").exists());
    // A retry after committing the changes succeeds cleanly.
    crate::git_test_support::git_run(&wt, &["add", "-A"]);
    crate::git_test_support::git_run(&wt, &["commit", "-qm", "preserve", "--no-verify"]);
    let dir = ensure(root, &implicit()).unwrap();
    assert!(dir.join(".balls/tasks/bl-legacytask.json").exists());
    assert!(dir.join(".balls/tasks/bl-untracked.json").exists());
}

#[test]
fn ensure_migration_clears_a_stray_unregistered_worktree_dir() {
    // The legacy `.balls/worktree` lingers as a plain directory that
    // git no longer tracks as a worktree — `git worktree remove`
    // fails, so the filesystem fallback clears it.
    let project = legacy_project();
    let root = project.path();
    crate::git_test_support::git_run(root, &["worktree", "remove", "--force", ".balls/worktree"]);
    fs::create_dir_all(root.join(".balls/worktree/stray")).unwrap();
    ensure(root, &implicit()).unwrap();
    assert!(!root.join(".balls/worktree").exists(), "the stray worktree dir is cleared");
}

#[test]
fn ensure_refuses_a_pre_existing_non_symlink_at_the_tasks_path() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".balls/tasks")).unwrap();
    let err = ensure(root.path(), &explicit(&hub_url(&hub))).unwrap_err();
    assert!(err.to_string().contains("unexpected non-symlink"), "{err}");
}

#[test]
fn ensure_absorbs_a_real_plugins_dir_into_the_checkout() {
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    let plugins = root.path().join(".balls/plugins");
    fs::create_dir_all(&plugins).unwrap();
    fs::write(plugins.join("github.json"), "{}\n").unwrap();
    fs::write(plugins.join(".gitkeep"), "").unwrap();
    let dir = ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();
    assert!(plugins.is_symlink(), "real plugins dir replaced by a symlink");
    assert!(dir.join(".balls/plugins/github.json").exists(), "config absorbed");
}

#[test]
fn ensure_commits_absorbed_plugin_files_to_the_tracker_branch() {
    // Regression for bl-73bb: legacy `.balls/plugins/*.json` files
    // absorbed during in-place migration must land on the tracker
    // branch, not dangle untracked in the state checkout — otherwise
    // they never push and a fresh clone does not inherit them.
    let hub = hub_repo();
    let root = tempfile::TempDir::new().unwrap();
    let plugins = root.path().join(".balls/plugins");
    fs::create_dir_all(&plugins).unwrap();
    fs::write(plugins.join("github.json"), "{\"k\":\"v\"}\n").unwrap();
    let dir = ensure(root.path(), &explicit(&hub_url(&hub))).unwrap();
    assert!(
        !git::has_uncommitted_changes(&dir).unwrap(),
        "absorbed plugin files must be committed, not left in the worktree"
    );
    let out = git::run_git_in(&dir, &["ls-tree", "-r", "HEAD", "--name-only"]).unwrap();
    let names = String::from_utf8_lossy(&out.stdout);
    assert!(
        names.lines().any(|n| n == ".balls/plugins/github.json"),
        "absorbed file is reachable from HEAD; got tree:\n{names}"
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
fn looks_like_url_rejects_bare_remote_names() {
    assert!(!looks_like_url("origin"));
    assert!(!looks_like_url("hub"));
    assert!(!looks_like_url("anything:1234"));
}

#[test]
fn run_at_propagates_non_zero_git_exit() {
    let dir = tempfile::TempDir::new().unwrap();
    let err = run_at(dir.path(), &["rev-parse", "--show-toplevel"]).unwrap_err();
    match err {
        BallError::Git(msg) => assert!(msg.contains("exited with") || msg.contains("rev-parse"), "{msg}"),
        other => panic!("expected Git error, got {other:?}"),
    }
}
