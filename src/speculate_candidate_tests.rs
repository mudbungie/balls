//! Unit tests for [`crate::speculate_candidate`] — real git, fixture repos,
//! and the no-leak invariants the design demands.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::{build_dir, commit_tree, merge_tree, remove_build_dir, Merge};

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// main with one file; `work/clean` edits a second file; `work/hostile` edits
/// the SAME line main later changed — a guaranteed conflict.
fn repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("shared"), "line\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "base"]);
    let base = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["checkout", "-q", "-b", "work/clean"]);
    fs::write(root.join("other"), "clean\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "clean"]);
    git(&root, &["checkout", "-q", "main"]);
    git(&root, &["checkout", "-q", "-b", "work/hostile", &base]);
    fs::write(root.join("shared"), "hostile\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "hostile"]);
    git(&root, &["checkout", "-q", "main"]);
    fs::write(root.join("shared"), "moved\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "moved"]);
    let main = git(&root, &["rev-parse", "HEAD"]);
    (tmp, root, main)
}

#[test]
fn clean_merge_yields_a_tree_and_conflict_says_so() {
    let (_tmp, root, main) = repo();
    let clean = git(&root, &["rev-parse", "work/clean"]);
    let hostile = git(&root, &["rev-parse", "work/hostile"]);
    match merge_tree(&root, &main, &clean).unwrap() {
        Merge::Tree(tree) => assert_eq!(tree.len(), 40, "a full tree oid"),
        Merge::Conflict => panic!("disjoint edits must merge clean"),
    }
    assert_eq!(merge_tree(&root, &main, &hostile).unwrap(), Merge::Conflict);
}

#[test]
fn merge_tree_speaks_gits_voice_on_a_broken_ref() {
    let (_tmp, root, main) = repo();
    let err = merge_tree(&root, &main, "refs/heads/work/ghost").unwrap_err();
    assert!(err.to_string().contains("git merge-tree"), "names the act: {err}");
}

#[test]
fn candidate_commits_are_real_ancestry_but_unreferenced() {
    let (_tmp, root, main) = repo();
    let clean = git(&root, &["rev-parse", "work/clean"]);
    let Merge::Tree(tree) = merge_tree(&root, &main, &clean).unwrap() else {
        panic!("clean merge expected");
    };
    let candidate = commit_tree(&root, &tree, &[&main, &clean]).unwrap();
    let parents = git(&root, &["log", "-1", "--format=%P", &candidate]);
    assert_eq!(parents, format!("{main} {clean}"), "both parents recorded");
    let holders = git(&root, &["for-each-ref", "--points-at", &candidate]);
    assert!(holders.is_empty(), "nothing may reference a candidate: {holders}");
    assert!(commit_tree(&root, "0000000000000000000000000000000000000000", &[]).is_err());
}

#[test]
fn build_dirs_exist_only_between_add_and_remove() {
    let (_tmp, root, main) = repo();
    let dir = root.parent().unwrap().join("build");
    build_dir(&root, &main, &dir).unwrap();
    assert!(dir.join("shared").exists(), "the candidate is materialized");
    remove_build_dir(&root, &dir).unwrap();
    assert!(!dir.exists(), "no debris after removal");
    let listing = git(&root, &["worktree", "list", "--porcelain"]);
    assert_eq!(listing.matches("worktree ").count(), 1, "only the real checkout: {listing}");
    assert!(build_dir(&root, "not-a-commit", &dir).is_err());
    assert!(remove_build_dir(&root, &dir).is_err(), "removing the never-added is loud");
}
