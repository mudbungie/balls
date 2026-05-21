//! Git plumbing for the state branch topology: orphan branch creation,
//! remote-tracking setup, and worktree attachment for an existing branch.
//! Separate from `git.rs` so that module stays under the 300-line cap.

use crate::error::{BallError, Result};
use crate::git::clean_git_command;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Stdio;

fn run(dir: &Path, args: &[&str]) -> Result<String> {
    let out = clean_git_command(dir)
        .args(args)
        .output()
        .map_err(|e| BallError::Git(format!("spawn git: {e}")))?;
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

/// Drop git's worktree registry entries whose checkout dir no longer
/// exists. Lets `bl init` re-materialize a hand-removed
/// `.balls/worktree/` without the operator having to know about the
/// stale registry — without this, a subsequent `worktree add` fails
/// with "missing but already registered worktree".
pub fn worktree_prune(dir: &Path) -> Result<()> {
    run(dir, &["worktree", "prune"])?;
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
            &format!("refs/heads/{branch}"),
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
            &format!("refs/remotes/{remote}/{branch}"),
        ],
    )
    .is_ok()
}

/// Create a local branch tracking `remote/branch`. Assumes the remote
/// branch already exists as a remote-tracking ref.
pub fn create_tracking_branch(dir: &Path, branch: &str, remote: &str) -> Result<()> {
    run(dir, &["branch", branch, &format!("{remote}/{branch}")])?;
    Ok(())
}

/// Return the subject line of every commit reachable from `refname`,
/// oldest first in output iteration order (git log gives newest first;
/// we reverse for stable order).
pub fn log_subjects(dir: &Path, refname: &str) -> Result<Vec<String>> {
    let out = run(dir, &["log", "--format=%s", refname])?;
    Ok(out.lines().map(String::from).collect())
}

/// Create an orphan branch pointing at a single empty-tree commit.
/// Uses `mktree` + `commit-tree` + `update-ref` so no working tree is
/// disturbed; safe to call from any checkout.
pub fn create_orphan_branch(dir: &Path, branch: &str, message: &str) -> Result<()> {
    let tree_out = clean_git_command(dir)
        .args(["mktree"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| BallError::Git(format!("git mktree spawn: {e}")))?;
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
        &["update-ref", &format!("refs/heads/{branch}"), &commit],
    )?;
    Ok(())
}

/// Task ids present under `.balls/tasks/` at `refname` (any tree-ish).
/// Notes sidecars and scaffolding files are filtered out.
pub fn ls_task_ids(dir: &Path, refname: &str) -> Result<BTreeSet<String>> {
    let out = run(dir, &["ls-tree", "--name-only", refname, ".balls/tasks/"])?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let name = l.rsplit('/').next().unwrap_or(l);
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect())
}

/// Contents of `path` at `refname`, or `None` if it does not exist
/// there. Used to compare a local task file against the hub's copy.
pub fn show_file(dir: &Path, refname: &str, path: &str) -> Result<Option<String>> {
    let out = clean_git_command(dir)
        .args(["show", &format!("{refname}:{path}")])
        .output()
        .map_err(|e| BallError::Git(format!("git show spawn: {e}")))?;
    Ok(out
        .status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).to_string()))
}

/// Re-root `branch` (checked out in `dir`) as a fresh parentless
/// commit carrying its current tree. The content is preserved; the
/// shared ancestry with any hub is severed, so the repo can no longer
/// fast-forward into — or be pushed onto — the hub. This is the
/// history half of `bl remaster --detach` (the config half clears
/// `state_remote`). The worktree is reset to the new commit so HEAD
/// and the index match.
pub fn reroot_orphan(dir: &Path, branch: &str, message: &str) -> Result<()> {
    let tree = run(dir, &["rev-parse", &format!("{branch}^{{tree}}")])?
        .trim()
        .to_string();
    let commit = run(dir, &["commit-tree", &tree, "-m", message])?
        .trim()
        .to_string();
    run(
        dir,
        &["update-ref", &format!("refs/heads/{branch}"), &commit],
    )?;
    run(dir, &["reset", "--hard", branch])?;
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
            other => panic!("expected Git error, got {other:?}"),
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        assert!(clean_git_command(dir).args(args).output().unwrap().status.success());
    }

    #[test]
    fn show_file_and_ls_task_ids_against_head() {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        git(p, &["init", "-q", "-b", "main"]);
        git(p, &["config", "user.email", "t@e.x"]);
        git(p, &["config", "user.name", "t"]);
        std::fs::create_dir_all(p.join(".balls/tasks")).unwrap();
        std::fs::write(p.join(".balls/tasks/bl-aaaa.json"), "JSON\n").unwrap();
        std::fs::write(p.join(".balls/tasks/.gitkeep"), "").unwrap();
        git(p, &["add", "-A"]);
        git(p, &["commit", "-qm", "x", "--no-verify"]);

        let got = show_file(p, "HEAD", ".balls/tasks/bl-aaaa.json").unwrap();
        assert_eq!(got.as_deref(), Some("JSON\n"));
        assert!(show_file(p, "HEAD", ".balls/tasks/missing.json")
            .unwrap()
            .is_none());

        let ids = ls_task_ids(p, "HEAD").unwrap();
        assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec!["bl-aaaa"]);
    }
}
