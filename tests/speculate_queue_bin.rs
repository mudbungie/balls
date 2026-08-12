//! E2E for `bl-speculate`'s queue verbs (bl-5c5f): spawn the real binary in a
//! throwaway repo and prove the plain-text queue contract agents script
//! against. Coverage-neutral file; the llvm engine attributes the spawned
//! binary's src lines here.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Sys;

use assert_cmd::Command;
use tempfile::TempDir;

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Sys::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repo with one commit and `work/a` + `work/b` branches.
fn repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    fs::write(root.join("f"), "one").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "c1"]);
    let c1 = git(&root, &["rev-parse", "HEAD"]);
    git(&root, &["branch", "work/a", &c1]);
    git(&root, &["branch", "work/b", &c1]);
    (tmp, root, c1)
}

fn speculate(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl-speculate").unwrap();
    cmd.current_dir(root);
    cmd
}

#[test]
fn enqueue_prints_the_sealed_tip_and_queue_lists_in_order() {
    let (_tmp, root, c1) = repo();
    let out = speculate(&root).arg("enqueue").arg("a").assert().success();
    let tip = String::from_utf8_lossy(&out.get_output().stdout).trim().to_string();
    assert_eq!(tip, c1);
    speculate(&root).arg("enqueue").arg("b").assert().success();
    let out = speculate(&root).arg("queue").assert().success();
    let listing = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    let lines: Vec<&str> = listing.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("1 ") && lines[0].contains(&c1), "positioned: {listing}");
    assert!(lines[1].starts_with("2 "), "positioned: {listing}");
}

#[test]
fn an_unsealed_entry_is_dashed_and_dequeue_removes() {
    let (_tmp, root, _c1) = repo();
    speculate(&root).arg("enqueue").arg("a").assert().success();
    git(&root, &["branch", "-D", "work/a"]);
    let out = speculate(&root).arg("queue").assert().success();
    let listing = String::from_utf8_lossy(&out.get_output().stdout).into_owned();
    assert!(listing.starts_with("- a "), "no position for the unsealed: {listing}");
    assert!(listing.trim_end().ends_with("unsealed"), "spoken plainly: {listing}");
    speculate(&root).arg("dequeue").arg("a").assert().success();
    let out = speculate(&root).arg("queue").assert().success();
    assert!(out.get_output().stdout.is_empty());
}

#[test]
fn queue_verbs_need_no_cache_environment() {
    let (_tmp, root, _c1) = repo();
    let mut cmd = speculate(&root);
    cmd.env_remove("HOME").arg("queue").assert().success();
}

#[test]
fn missing_id_is_a_usage_error() {
    let (_tmp, root, _c1) = repo();
    for verb in ["enqueue", "dequeue"] {
        let out = speculate(&root).arg(verb).assert().code(1).get_output().clone();
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("usage: bl-speculate"),
            "{verb} without an id must speak usage"
        );
    }
}

#[test]
fn enqueue_of_a_missing_branch_carries_gits_voice() {
    let (_tmp, root, _c1) = repo();
    let out = speculate(&root).arg("enqueue").arg("ghost").assert().code(1).get_output().clone();
    assert!(String::from_utf8_lossy(&out.stderr).contains("bl-speculate:"));
}
