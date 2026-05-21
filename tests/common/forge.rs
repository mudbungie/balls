//! Shared helpers for deferred-mode / forge-gated delivery tests
//! (SPEC-forge-gated-delivery): seeding deferred config, resolving the
//! auto-opened `forge-gate` child, and reading refs.

#![allow(dead_code)]

use super::{bl, git};
use std::fs;
use std::path::Path;

/// Seed `.balls/config.json` before `bl init` so the repo is in the
/// requested delivery mode from its first lifecycle command (mirrors
/// `tests/target_branch.rs::seed_config`).
pub fn seed(repo: &Path, target_branch: Option<&str>) {
    let balls = repo.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    let tb = match target_branch {
        Some(b) => format!(r#","target_branch":"{b}""#),
        None => String::new(),
    };
    fs::write(
        balls.join("config.json"),
        format!(
            r#"{{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"{tb},"delivery":{{"mode":"deferred"}}}}"#
        ),
    )
    .unwrap();
}

pub fn sha(repo: &Path, refname: &str) -> String {
    git(repo, &["rev-parse", refname]).trim().to_string()
}

/// The id of the lone `forge-gate` child, via `bl list --tag`.
pub fn gate_child(repo: &Path) -> String {
    let out = bl(repo)
        .args(["list", "--json", "--tag", "forge-gate"])
        .output()
        .expect("bl list");
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("list json");
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1, "exactly one forge-gate child");
    arr[0]["id"].as_str().unwrap().to_string()
}
