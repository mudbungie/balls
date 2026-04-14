//! Git plumbing for the state branch topology: orphan branch creation,
//! remote-tracking setup, and worktree attachment for an existing branch.
//! Separate from `git.rs` so that module stays under the 300-line cap.

use crate::error::{BallError, Result};
use crate::git::clean_git_command;
use std::path::Path;
use std::process::Stdio;

fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let out = clean_git_command(dir)
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

/// Return the subject line of every commit reachable from `refname`,
/// oldest first in output iteration order (git log gives newest first;
/// we reverse for stable order).
pub fn log_subjects(dir: &Path, refname: &str) -> Result<Vec<String>> {
    let out = run(dir, &["log", "--format=%s", refname])?;
    Ok(out.lines().map(|l| l.to_string()).collect())
}

/// Create an orphan branch pointing at a single empty-tree commit.
/// Uses `mktree` + `commit-tree` + `update-ref` so no working tree is
/// disturbed; safe to call from any checkout.
pub fn create_orphan_branch(dir: &Path, branch: &str, message: &str) -> Result<()> {
    let tree_out = clean_git_command(dir)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_orphan_branch_in_non_git_dir_errors() {
        let dir = TempDir::new().unwrap();
        let err = create_orphan_branch(dir.path(), "any", "msg").unwrap_err();
        match err {
            BallError::Git(_) => {}
            other => panic!("expected Git error, got {:?}", other),
        }
    }
}
