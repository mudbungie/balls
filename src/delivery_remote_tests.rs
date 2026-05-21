//! Unit tests for `delivery_remote`. A throwaway local git repo stands
//! in for the "delivered" code repo so the resolve path can exercise
//! the fetch + tag scan against a real git history without a network
//! dependency. A bogus URL covers the soft-fail / cache-teardown path.

use super::*;
use crate::task::{NewTaskOpts, Task};
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

/// Build a non-bare repo with a single commit whose subject carries
/// `[bl-id]`. Returns the path; the SHA of that commit is what `resolve`
/// should produce when given the path as `delivered_repo`.
fn repo_with_tag_commit(id: &str) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    run(dir.path(), &["init", "-q", "-b", "main"]);
    run(dir.path(), &["config", "user.email", "t@e.x"]);
    run(dir.path(), &["config", "user.name", "t"]);
    run(dir.path(), &["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    run(dir.path(), &["add", "a.txt"]);
    run(dir.path(), &["commit", "-qm", &format!("seed [{id}]")]);
    let sha = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    (dir, sha)
}

fn task_with_id(id: &str) -> Task {
    Task::new(
        NewTaskOpts {
            title: "t".into(),
            ..Default::default()
        },
        id.into(),
    )
}

#[test]
fn cache_dir_for_is_under_repo_root_and_deterministic() {
    let root = Path::new("/x/y");
    let a = cache_dir_for(root, "git@h:a.git");
    let b = cache_dir_for(root, "git@h:a.git");
    let c = cache_dir_for(root, "git@h:b.git");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert!(a.starts_with(root.join(CODE_REFS_REL)));
    assert!(a.to_string_lossy().ends_with(".git"));
}

#[test]
fn resolve_finds_tag_commit_via_fresh_fetch() {
    let (src, sha) = repo_with_tag_commit("bl-fa11");
    let url = src.path().to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    let task = task_with_id("bl-fa11");

    let got = resolve(root.path(), &url, &task).expect("remote resolve hits");
    assert_eq!(got, sha);
    assert!(
        cache_dir_for(root.path(), &url).join("HEAD").exists(),
        "cache should be materialized for re-use",
    );
}

#[test]
fn resolve_returns_none_when_tag_absent() {
    let (src, _sha) = repo_with_tag_commit("bl-fa11");
    let url = src.path().to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    // Different id — no commit in src carries this tag.
    let task = task_with_id("bl-9999");

    assert!(resolve(root.path(), &url, &task).is_none());
}

#[test]
fn resolve_unreachable_url_soft_fails_and_tears_cache_down() {
    let root = TempDir::new().unwrap();
    let url = "/no/such/path/repo.git";
    let task = task_with_id("bl-abcd");

    assert!(resolve(root.path(), url, &task).is_none());
    assert!(
        !cache_dir_for(root.path(), url).exists(),
        "first-time fetch failure must remove the half-built scaffold",
    );
}

#[test]
fn warm_cache_refresh_succeeds_on_repeat_call() {
    // A second resolve against the same URL exercises the warm-cache
    // refresh path (git fetch origin) and must produce the same sha
    // without re-cloning.
    let (src, sha) = repo_with_tag_commit("bl-fa11");
    let url = src.path().to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    let task = task_with_id("bl-fa11");

    assert_eq!(resolve(root.path(), &url, &task), Some(sha.clone()));
    assert_eq!(resolve(root.path(), &url, &task), Some(sha));
}

#[test]
fn clone_failure_when_cache_path_is_a_regular_file_soft_fails() {
    // Pre-occupy the cache path with a regular file. `git clone --bare`
    // refuses ("destination ... already exists and is not an empty
    // directory"), so the first-time path exits via the soft-fail
    // teardown branch.
    let root = TempDir::new().unwrap();
    let url = "/anything";
    let cache = cache_dir_for(root.path(), url);
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    std::fs::write(&cache, "junk").unwrap();

    let task = task_with_id("bl-abcd");
    assert!(resolve(root.path(), url, &task).is_none());
}

#[test]
fn warm_cache_serves_when_origin_disappears() {
    // Materialize a cache against a reachable URL, then move the
    // source out from under it. The next call should still resolve the
    // sha from the warm cache (with a note about the failed refresh)
    // rather than tear the cache down.
    let (src, sha) = repo_with_tag_commit("bl-fa11");
    let url = src.path().to_string_lossy().into_owned();
    let root = TempDir::new().unwrap();
    let task = task_with_id("bl-fa11");

    assert_eq!(resolve(root.path(), &url, &task), Some(sha.clone()));

    // Drop the source so the next fetch fails.
    drop(src);

    let cache = cache_dir_for(root.path(), &url);
    assert!(cache.exists(), "warm cache must survive");
    assert_eq!(
        resolve(root.path(), &url, &task),
        Some(sha),
        "warm cache should still answer the same tag scan",
    );
}
