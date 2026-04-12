//! Git plumbing for the state branch topology: orphan branch creation,
//! remote-tracking setup, and worktree attachment for an existing branch.
//! Separate from `git.rs` so that module stays under the 300-line cap.

use crate::error::{BallError, Result};
use std::path::Path;
use std::process::{Command, Stdio};

fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| BallError::Git(format!("spawn git: {}", e)))?;
    if !out.status.success() {
        return Err(BallError::Git(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Check out an existing branch into a new worktree.
pub fn worktree_add_existing(dir: &Path, path: &Path, branch: &str) -> Result<()> {
    run(
        dir,
        &["worktree", "add", &path.to_string_lossy(), branch],
    )?;
    Ok(())
}

/// True if `branch` exists locally.
pub fn branch_exists(dir: &Path, branch: &str) -> bool {
    run(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{}", branch),
        ],
    )
    .is_ok()
}

/// True if `remote/branch` exists as a remote-tracking ref.
pub fn has_remote_branch(dir: &Path, remote: &str, branch: &str) -> bool {
    run(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/remotes/{}/{}", remote, branch),
        ],
    )
    .is_ok()
}

/// Create a local branch tracking `remote/branch`. Assumes the remote
/// branch already exists as a remote-tracking ref.
pub fn create_tracking_branch(dir: &Path, branch: &str, remote: &str) -> Result<()> {
    run(dir, &["branch", branch, &format!("{}/{}", remote, branch)])?;
    Ok(())
}

/// Create an orphan branch pointing at a single empty-tree commit.
/// Uses `mktree` + `commit-tree` + `update-ref` so no working tree is
/// disturbed; safe to call from any checkout.
pub fn create_orphan_branch(dir: &Path, branch: &str, message: &str) -> Result<()> {
    let tree_out = Command::new("git")
        .current_dir(dir)
        .args(["mktree"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| BallError::Git(format!("git mktree spawn: {}", e)))?;
    if !tree_out.status.success() {
        return Err(BallError::Git(format!(
            "git mktree: {}",
            String::from_utf8_lossy(&tree_out.stderr).trim()
        )));
    }
    let empty_tree = String::from_utf8_lossy(&tree_out.stdout).trim().to_string();
    let commit = run(dir, &["commit-tree", &empty_tree, "-m", message])?
        .trim()
        .to_string();
    run(
        dir,
        &["update-ref", &format!("refs/heads/{}", branch), &commit],
    )?;
    Ok(())
}
