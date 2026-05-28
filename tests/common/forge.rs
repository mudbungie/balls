//! Shared helpers for deferred-mode / forge-gated delivery tests
//! (SPEC-forge-gated-delivery): seeding deferred config, resolving the
//! auto-opened `forge-gate` child, and reading refs.

#![allow(dead_code)]

use super::{bl, edit_and_commit_repo_config, git, set_default_target_branch};
use std::path::Path;

/// Configure the clone for deferred (forge-PR) delivery, layout-aware.
///
/// SPEC §6.5: the live `bl init` writes only XDG state — the legacy
/// pre-init `.balls/config.json` seed is no longer read. So this
/// helper runs `bl init` itself (idempotent on a warm tracker), then
/// edits + commits `repo.json` on the tracker branch with
/// `integrate.mode = forge-pr`. The optional `target_branch` is
/// per-task under the revised SPEC §6.7 (the repo-level field was
/// removed); the helper records it as the thread-local
/// `DEFAULT_TARGET_BRANCH` so subsequent `create_task` /
/// `create_task_full` calls forward `--target-branch` automatically.
pub fn seed(repo: &Path, target_branch: Option<&str>) {
    bl(repo).arg("init").assert().success();
    edit_and_commit_repo_config(repo, "balls: configure deferred mode", |c| {
        let obj = c.as_object_mut().expect("repo.json is an object");
        obj.insert(
            "integrate".into(),
            serde_json::json!({ "mode": "forge-pr" }),
        );
    });
    set_default_target_branch(target_branch.map(String::from));
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
