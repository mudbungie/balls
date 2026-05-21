//! Unit tests for the leaf helpers in `review_safety`. The end-to-end
//! transactional behavior of `commit_squash_and_flip` is exercised by
//! `tests/review.rs`; this file targets the small leaf helpers and
//! their failure paths so coverage stays at 100%.

use super::*;
use crate::git_test_support::{git_run, git_stdout, init_repo};
use crate::store::Store;
use tempfile::tempdir;

#[test]
fn add_user_changes_stages_normal_files() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    std::fs::write(td.path().join("a.txt"), "hi").unwrap();
    add_user_changes(td.path()).unwrap();
    let staged = git_stdout(td.path(), &["diff", "--cached", "--name-only"]);
    assert!(staged.contains("a.txt"), "got: {staged}");
}

#[test]
fn add_user_changes_excludes_balls_runtime_paths() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    std::fs::create_dir_all(td.path().join(".balls/local")).unwrap();
    std::fs::create_dir_all(td.path().join(".balls/tasks")).unwrap();
    std::fs::create_dir_all(td.path().join(".balls/worktree")).unwrap();
    std::fs::write(td.path().join(".balls/local/lock"), "x").unwrap();
    std::fs::write(td.path().join(".balls/tasks/t.json"), "x").unwrap();
    std::fs::write(td.path().join(".balls/worktree/x"), "x").unwrap();
    std::fs::write(td.path().join("user.txt"), "ok").unwrap();
    add_user_changes(td.path()).unwrap();
    let staged = git_stdout(td.path(), &["diff", "--cached", "--name-only"]);
    assert!(staged.contains("user.txt"), "got: {staged}");
    for p in crate::runtime_paths::backstop_paths() {
        assert!(
            !staged.lines().any(|l| l.starts_with(p)),
            "runtime path {p} leaked into staging: {staged}"
        );
    }
}

#[test]
fn commit_touches_runtime_flags_runtime_paths() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    std::fs::create_dir_all(td.path().join(".balls/local")).unwrap();
    std::fs::write(td.path().join(".balls/local/lock"), "x").unwrap();
    git_run(td.path(), &["add", "-A"]);
    git_run(td.path(), &["commit", "-m", "bad"]);
    let sha = git_stdout(td.path(), &["rev-parse", "HEAD"]);
    let hits = commit_touches_runtime(td.path(), &sha).unwrap();
    assert_eq!(hits, vec![".balls/local/lock".to_string()]);
}

#[test]
fn commit_touches_runtime_flags_code_refs_cache() {
    // bl-c4e2: the `--resolve-remote` code-refs cache shares the
    // shape of the other runtime paths — a deep file under it
    // (here `.git/HEAD` inside a forge clone dir) must still be
    // recognized as runtime so a stale-gitignore repo cannot
    // accidentally squash the cache into the integration branch.
    let td = tempdir().unwrap();
    init_repo(td.path());
    std::fs::create_dir_all(td.path().join(".balls/code-refs/foo.git")).unwrap();
    std::fs::write(td.path().join(".balls/code-refs/foo.git/HEAD"), "ref: x").unwrap();
    git_run(td.path(), &["add", "-A"]);
    git_run(td.path(), &["commit", "-m", "bad"]);
    let sha = git_stdout(td.path(), &["rev-parse", "HEAD"]);
    let hits = commit_touches_runtime(td.path(), &sha).unwrap();
    assert_eq!(hits, vec![".balls/code-refs/foo.git/HEAD".to_string()]);
}

#[test]
fn commit_touches_runtime_flags_state_repo_clone() {
    // bl-c439: master_url mode materializes a balls-owned clone of the
    // task hub at `.balls/state-repo/`. It shares the shape of the
    // other runtime paths — a deep file under it (here a task JSON in
    // the hub clone's checked-out tree) must be recognized as runtime
    // so a stale-gitignore repo cannot squash the entire hub clone into
    // the integration branch.
    let td = tempdir().unwrap();
    init_repo(td.path());
    std::fs::create_dir_all(td.path().join(".balls/state-repo/.balls/tasks")).unwrap();
    std::fs::write(
        td.path().join(".balls/state-repo/.balls/tasks/bl-9999.json"),
        "{}",
    )
    .unwrap();
    git_run(td.path(), &["add", "-A"]);
    git_run(td.path(), &["commit", "-m", "bad"]);
    let sha = git_stdout(td.path(), &["rev-parse", "HEAD"]);
    let hits = commit_touches_runtime(td.path(), &sha).unwrap();
    assert_eq!(
        hits,
        vec![".balls/state-repo/.balls/tasks/bl-9999.json".to_string()]
    );
}

#[test]
fn commit_touches_runtime_empty_for_clean_commit() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    std::fs::write(td.path().join("ok.txt"), "x").unwrap();
    git_run(td.path(), &["add", "-A"]);
    git_run(td.path(), &["commit", "-m", "ok"]);
    let sha = git_stdout(td.path(), &["rev-parse", "HEAD"]);
    let hits = commit_touches_runtime(td.path(), &sha).unwrap();
    assert!(hits.is_empty(), "expected empty, got {hits:?}");
}


#[test]
fn commit_squash_and_flip_rewinds_main_when_squash_carries_runtime() {
    // Defense-in-depth path: if a work branch tip somehow carries a
    // runtime file all the way past the wip step (here, by skipping
    // `add_user_changes` entirely and committing the runtime path
    // directly on the work branch off a base that doesn't have it),
    // the post-squash check must reject the review and rewind main.
    let td = tempdir().unwrap();
    init_repo(td.path());
    let store = Store::init(td.path(), false, None).unwrap();
    let pre_main = git::git_resolve_sha(td.path(), "HEAD").unwrap();

    // Build a work branch off main with a single commit that tracks
    // a runtime path. Bypasses `add_user_changes`, which is exactly
    // the scenario the post-squash check is the backstop for.
    git_run(td.path(), &["checkout", "-q", "-b", "work/bl-test"]);
    std::fs::create_dir_all(td.path().join(".balls/local")).unwrap();
    std::fs::write(td.path().join(".balls/local/x"), "x").unwrap();
    git_run(td.path(), &["add", "-f", ".balls/local/x"]);
    git_run(td.path(), &["commit", "-m", "runtime"]);
    git_run(td.path(), &["checkout", "-q", "main"]);

    let err = commit_squash_and_flip(
        &store,
        "bl-test",
        "work/bl-test",
        "msg [bl-test]",
        None,
        "test",
        &pre_main,
        "main",
    )
    .unwrap_err();
    assert!(
        matches!(&err, BallError::Other(s) if s.contains(".balls/local")),
        "{err:?}"
    );
    let post_main = git::git_resolve_sha(td.path(), "HEAD").unwrap();
    assert_eq!(
        pre_main, post_main,
        "main should be rewound after rejection"
    );
}

#[test]
fn rewind_main_uses_update_ref_on_bare_layout() {
    // Bare-repo path of rewind_main: a working-tree `reset --hard`
    // would fail because there is no working tree, so the helper
    // routes through `update-ref` instead. Build a Store, advance
    // main one commit, flip the repo to bare, then rewind and
    // verify main is back at the pre-advance tip.
    let td = tempdir().unwrap();
    init_repo(td.path());
    let store = Store::init(td.path(), false, None).unwrap();
    let pre = git::git_resolve_sha(td.path(), "HEAD").unwrap();

    git_run(td.path(), &["commit", "--allow-empty", "-m", "advance"]);
    let advanced = git::git_resolve_sha(td.path(), "HEAD").unwrap();
    assert_ne!(pre, advanced, "sanity: HEAD advanced");

    git_run(td.path(), &["config", "core.bare", "true"]);
    rewind_main(&store, "main", &pre).unwrap();
    let after = git::git_resolve_sha(td.path(), "refs/heads/main").unwrap();
    assert_eq!(pre, after, "bare rewind must move main back via update-ref");
}

#[test]
fn runtime_in_squash_error_pluralizes_correctly() {
    let one = runtime_in_squash_error("bl-aaaa", &[".balls/local".into()]);
    assert!(
        matches!(&one, BallError::Other(s) if s.contains("path .balls/local")),
        "{one:?}"
    );
    let many = runtime_in_squash_error("bl-bbbb", &[".balls/local".into(), ".balls/tasks".into()]);
    assert!(
        matches!(&many, BallError::Other(s) if s.contains("paths .balls/local, .balls/tasks")),
        "{many:?}"
    );
}

#[test]
fn runtime_in_squash_error_lists_backstop_subpaths() {
    // The help-text example list must derive from the runtime_paths
    // table, not a literal — so a new backstop sidecar (e.g. the
    // `.balls/state-repo` the old literal silently omitted) shows up
    // here automatically (bl-0151).
    let err = runtime_in_squash_error("bl-cccc", &[".balls/state-repo".into()]);
    for p in crate::runtime_paths::backstop_paths() {
        let sub = p.strip_prefix(".balls/").unwrap_or(p);
        assert!(
            matches!(&err, BallError::Other(s) if s.contains(&format!("`{sub}`"))),
            "missing `{sub}` in {err:?}",
        );
    }
}
