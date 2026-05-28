//! Git plumbing for the state branch topology: orphan branch creation,
//! remote-tracking setup, and worktree attachment for an existing branch.
//! Separate from `git.rs` so that module stays under the 300-line cap.

use crate::error::{BallError, Result};
use crate::git::{clean_git_command, run_git_ok};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Stdio;

/// Resolved fetch URL of remote `remote` in `dir`, or `None` when no
/// such remote exists. Works in a bare repo (reads `.git/config`).
pub fn remote_url(dir: &Path, remote: &str) -> Option<String> {
    let url = run_git_ok(dir, &["remote", "get-url", remote]).ok()?;
    let url = url.trim();
    (!url.is_empty()).then(|| url.to_string())
}

/// Point remote `remote` in `dir` at `url` — add it if absent,
/// re-point it if present.
pub fn set_remote(dir: &Path, remote: &str, url: &str) -> Result<()> {
    let verb = if crate::git::git_has_remote(dir, remote) { "set-url" } else { "add" };
    run_git_ok(dir, &["remote", verb, remote, url])?;
    Ok(())
}

/// Remove remote `remote` from `dir`. A missing remote is a no-op.
pub fn remove_remote(dir: &Path, remote: &str) {
    if crate::git::git_has_remote(dir, remote) {
        let _ = run_git_ok(dir, &["remote", "remove", remote]);
    }
}

/// Drop git's worktree registry entries whose checkout dir no longer
/// exists. Lets `bl init` re-materialize a hand-removed
/// `.balls/worktree/` without the operator having to know about the
/// stale registry — without this, a subsequent `worktree add` fails
/// with "missing but already registered worktree".
pub fn worktree_prune(dir: &Path) -> Result<()> {
    run_git_ok(dir, &["worktree", "prune"])?;
    Ok(())
}

/// True if `branch` exists locally.
pub fn branch_exists(dir: &Path, branch: &str) -> bool {
    run_git_ok(
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
    run_git_ok(
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
    run_git_ok(dir, &["branch", branch, &format!("{remote}/{branch}")])?;
    Ok(())
}

/// Return the subject line of every commit reachable from `refname`,
/// oldest first in output iteration order (git log gives newest first;
/// we reverse for stable order).
pub fn log_subjects(dir: &Path, refname: &str) -> Result<Vec<String>> {
    let out = run_git_ok(dir, &["log", "--format=%s", refname])?;
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
    let commit = run_git_ok(dir, &["commit-tree", &empty_tree, "-m", message])?
        .trim()
        .to_string();
    run_git_ok(
        dir,
        &["update-ref", &format!("refs/heads/{branch}"), &commit],
    )?;
    Ok(())
}

/// Task ids present under `.balls/tasks/` at `refname` (any tree-ish).
/// Notes sidecars and scaffolding files are filtered out.
pub fn ls_task_ids(dir: &Path, refname: &str) -> Result<BTreeSet<String>> {
    let out = run_git_ok(dir, &["ls-tree", "--name-only", refname, ".balls/tasks/"])?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let name = l.rsplit('/').next().unwrap_or(l);
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect())
}

/// Contents of `path` at `refname`, or `None` if it does not exist
/// there. Used to compare a local task file against the tracker's copy.
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

    #[test]
    fn remote_url_set_remote_and_remove_remote_round_trip() {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        git(p, &["init", "-q"]);

        // No remote yet.
        assert_eq!(remote_url(p, "origin"), None);
        // set_remote adds it when absent...
        set_remote(p, "origin", "https://a/x.git").unwrap();
        assert_eq!(remote_url(p, "origin").as_deref(), Some("https://a/x.git"));
        // ...and re-points it when present.
        set_remote(p, "origin", "https://b/y.git").unwrap();
        assert_eq!(remote_url(p, "origin").as_deref(), Some("https://b/y.git"));
        // remove_remote drops it; a second call is a silent no-op.
        remove_remote(p, "origin");
        assert_eq!(remote_url(p, "origin"), None);
        remove_remote(p, "origin");
    }
}
