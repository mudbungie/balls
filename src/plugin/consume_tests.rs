//! Unit coverage for the command-level consumption seam. The
//! end-to-end required-veto / override-log / sync_status behavior is
//! pinned through the real binary in `tests/plugin_*`; this file
//! targets the leaf helpers and every failure branch so coverage
//! stays at 100%.

use super::*;
use crate::git_test_support::{git_run, git_stdout};
use crate::participant::Event;
use crate::participant_config::InvocationOverrides;
use crate::store::Store;
use crate::task::{NewTaskOpts, Task};
use std::path::Path;
use tempfile::tempdir;

fn init_repo(path: &Path) {
    git_run(path, &["init", "-q", "-b", "main"]);
    git_run(path, &["config", "user.email", "t@e.com"]);
    git_run(path, &["config", "user.name", "t"]);
    git_run(path, &["config", "commit.gpgsign", "false"]);
    git_run(path, &["commit", "--allow-empty", "-m", "init"]);
}

fn git_store() -> (tempfile::TempDir, Store) {
    let td = tempdir().unwrap();
    init_repo(td.path());
    let store = Store::init(td.path(), false, None).unwrap();
    (td, store)
}

fn stealth_store() -> (tempfile::TempDir, Store) {
    let td = tempdir().unwrap();
    let store = Store::init(
        td.path(),
        true,
        Some(td.path().join("t").to_string_lossy().into_owned()),
    )
    .unwrap();
    (td, store)
}

fn task(store: &Store, id: &str) -> Task {
    let t = Task::new(
        NewTaskOpts {
            title: "x".into(),
            ..Default::default()
        },
        id.into(),
    );
    store.save_task(&t).unwrap();
    t
}

#[test]
fn amended_message_subject_only() {
    assert_eq!(amended_message("create [bl-1]", " [--no-sync]"), "create [bl-1] [--no-sync]");
}

#[test]
fn amended_message_preserves_body() {
    assert_eq!(
        amended_message("subj [bl-1]\n\nbody line", " [--skip=j]"),
        "subj [bl-1] [--skip=j]\n\nbody line"
    );
}

#[test]
fn amended_message_empty_input() {
    assert_eq!(amended_message("", " [--skip=j]"), " [--skip=j]");
}

#[test]
fn state_head_is_none_in_stealth() {
    let (_td, store) = stealth_store();
    assert!(state_head(&store).unwrap().is_none());
}

#[test]
fn state_head_resolves_in_git() {
    let (_td, store) = git_store();
    let sha = state_head(&store).unwrap().expect("git store has a state HEAD");
    assert_eq!(sha.len(), 40, "full sha: {sha}");
}

#[test]
fn log_overrides_noop_on_empty_tokens() {
    let (_td, store) = git_store();
    let before = state_head(&store).unwrap().unwrap();
    log_overrides(&store, &[]).unwrap();
    assert_eq!(state_head(&store).unwrap().unwrap(), before, "no amend without tokens");
}

#[test]
fn log_overrides_noop_in_stealth() {
    let (_td, store) = stealth_store();
    log_overrides(&store, &["--skip=jira".into()]).unwrap();
}

#[test]
fn log_overrides_amends_state_subject() {
    let (_td, store) = git_store();
    let sd = store.state_worktree_dir();
    log_overrides(&store, &["--no-sync".into(), "--skip=jira".into()]).unwrap();
    let subj = git_stdout(&sd, &["log", "-1", "--format=%s"]);
    assert!(subj.contains("[--no-sync] [--skip=jira]"), "got: {subj}");
}

#[test]
fn log_overrides_errors_when_amend_fails() {
    // Sabotage: an unborn branch has no HEAD to amend, so the
    // `git commit --amend` returns non-zero and the `!st.success()`
    // guard fires.
    let (_td, store) = git_store();
    let sd = store.state_worktree_dir();
    git_run(&sd, &["checkout", "--orphan", "void"]);
    let err = log_overrides(&store, &["--skip=jira".into()]).unwrap_err();
    assert!(format!("{err}").contains("override log"), "got: {err}");
}

#[test]
fn finish_ok_when_no_plugins() {
    let (_td, store) = git_store();
    let t = task(&store, "bl-1234");
    let rb = state_head(&store).unwrap();
    let out = finish(
        &store,
        Some(&t),
        &t,
        Event::Update,
        "alice",
        &InvocationOverrides::default(),
        &[],
        Rollback::State(rb.as_deref()),
    )
    .unwrap();
    assert!(out.skipped.is_empty());
}

#[test]
fn finish_rolls_state_back_on_dispatch_error() {
    let (_td, store) = git_store();
    let t = task(&store, "bl-1234");
    let rb = state_head(&store).unwrap();
    // Advance the state branch so a rewind to `rb` is observable,
    // then break config so `dispatch_push` errors at config load.
    let sd = store.state_worktree_dir();
    git_run(&sd, &["commit", "--allow-empty", "-m", "advance"]);
    std::fs::write(store.config_path(), "not json").unwrap();
    let err = finish(
        &store,
        None,
        &t,
        Event::Update,
        "a",
        &InvocationOverrides::default(),
        &[],
        Rollback::State(rb.as_deref()),
    )
    .unwrap_err();
    let _ = format!("{err}");
    assert_eq!(
        state_head(&store).unwrap().as_deref(),
        rb.as_deref(),
        "required failure rewinds the state branch"
    );
}

#[test]
fn finish_error_with_state_none_is_noop_rollback() {
    let (_td, store) = stealth_store();
    let t = task(&store, "bl-1234");
    std::fs::write(store.config_path(), "not json").unwrap();
    let err = finish(
        &store,
        None,
        &t,
        Event::Update,
        "a",
        &InvocationOverrides::default(),
        &[],
        Rollback::State(None),
    )
    .unwrap_err();
    let _ = format!("{err}");
}

#[test]
fn finish_error_with_dropclaim_attempts_unclaim() {
    // No worktree exists, so `drop_worktree` itself errors; `finish`
    // swallows that and still surfaces the dispatch error. Covers the
    // `Rollback::DropClaim` arm.
    let (_td, store) = git_store();
    let t = task(&store, "bl-1234");
    std::fs::write(store.config_path(), "not json").unwrap();
    let err = finish(
        &store,
        None,
        &t,
        Event::Claim,
        "a",
        &InvocationOverrides::default(),
        &[],
        Rollback::DropClaim,
    )
    .unwrap_err();
    let _ = format!("{err}");
}
