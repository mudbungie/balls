use crate::error::{BallError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

fn run_git_in(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| BallError::Git(format!("failed to spawn git: {}", e)))?;
    Ok(out)
}

fn run_git_ok(dir: &Path, args: &[&str]) -> Result<String> {
    let out = run_git_in(dir, args)?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(BallError::Git(format!(
            "git {}: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn git_root(from: &Path) -> Result<PathBuf> {
    let out = run_git_in(from, &["rev-parse", "--show-toplevel"])?;
    if !out.status.success() {
        return Err(BallError::NotARepo);
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(PathBuf::from(s))
}

pub fn git_common_dir(from: &Path) -> Result<PathBuf> {
    // Returns the .git directory shared across worktrees
    let out = run_git_ok(from, &["rev-parse", "--git-common-dir"])?;
    let p = PathBuf::from(out.trim());
    if p.is_absolute() {
        Ok(p)
    } else {
        Ok(from.join(p))
    }
}

pub fn git_add(dir: &Path, paths: &[&Path]) -> Result<()> {
    let mut args = vec!["add", "--"];
    let strs: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
    for s in &strs {
        args.push(s.as_str());
    }
    run_git_ok(dir, &args)?;
    Ok(())
}

pub fn git_rm(dir: &Path, paths: &[&Path]) -> Result<()> {
    let mut args = vec!["rm", "--"];
    let strs: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
    for s in &strs {
        args.push(s.as_str());
    }
    run_git_ok(dir, &args)?;
    Ok(())
}

pub fn git_add_all(dir: &Path) -> Result<()> {
    run_git_ok(dir, &["add", "-A"])?;
    Ok(())
}

pub fn git_commit(dir: &Path, message: &str) -> Result<()> {
    // Always commit during a merge (MERGE_HEAD exists), even if the index
    // matches HEAD — we need the merge commit to record both parents.
    if !has_staged_changes(dir)? && !is_merging(dir) {
        return Ok(());
    }
    run_git_ok(dir, &["commit", "-m", message, "--no-verify"])?;
    Ok(())
}

pub fn is_merging(dir: &Path) -> bool {
    // git_commit is only called from within balls-managed repos, so
    // git_common_dir always succeeds here; we treat a failure (genuine I/O
    // weirdness) as "not merging" so the caller skips the merge-finalize
    // path.
    git_common_dir(dir)
        .map(|c| c.join("MERGE_HEAD").exists())
        .unwrap_or(false)
}

pub fn has_staged_changes(dir: &Path) -> Result<bool> {
    let out = run_git_in(dir, &["diff", "--cached", "--quiet"])?;
    // exit 0 => no changes, exit 1 => changes
    Ok(!out.status.success())
}

pub fn has_uncommitted_changes(dir: &Path) -> Result<bool> {
    let out = run_git_ok(dir, &["status", "--porcelain"])?;
    Ok(!out.trim().is_empty())
}

pub fn git_fetch(dir: &Path, remote: &str) -> Result<bool> {
    let out = run_git_in(dir, &["fetch", remote])?;
    Ok(out.status.success())
}

#[derive(Debug, PartialEq, Eq)]
pub enum MergeResult {
    Clean,
    UpToDate,
    Conflict,
}

pub fn git_merge(dir: &Path, branch: &str, message: Option<&str>) -> Result<MergeResult> {
    git_merge_inner(dir, branch, message, false)
}

/// Merge with --no-ff: always create a merge commit even if fast-forward is possible.
pub fn git_merge_no_ff(dir: &Path, branch: &str, message: Option<&str>) -> Result<MergeResult> {
    git_merge_inner(dir, branch, message, true)
}

fn git_merge_inner(dir: &Path, branch: &str, message: Option<&str>, no_ff: bool) -> Result<MergeResult> {
    let mut args = vec!["merge", "--no-edit"];
    if no_ff {
        args.push("--no-ff");
    }
    if let Some(m) = message {
        args.push("-m");
        args.push(m);
    }
    args.push(branch);
    let out = run_git_in(dir, &args)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        if stdout.contains("Already up to date") || stdout.contains("Already up-to-date") {
            Ok(MergeResult::UpToDate)
        } else {
            Ok(MergeResult::Clean)
        }
    } else {
        let combined = format!("{}{}", stdout, stderr);
        if combined.contains("CONFLICT") || combined.contains("conflict") {
            Ok(MergeResult::Conflict)
        } else {
            Err(BallError::Git(format!(
                "merge failed: {}",
                combined.trim()
            )))
        }
    }
}

pub fn git_push(dir: &Path, remote: &str, branch: &str) -> Result<()> {
    run_git_ok(dir, &["push", remote, branch])?;
    Ok(())
}

pub fn git_worktree_add(dir: &Path, path: &Path, branch: &str) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    run_git_ok(dir, &["worktree", "add", &path_str, "-b", branch])?;
    Ok(())
}

pub fn git_worktree_remove(dir: &Path, path: &Path, force: bool) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path_str);
    run_git_ok(dir, &args)?;
    Ok(())
}

pub fn git_branch_delete(dir: &Path, branch: &str, force: bool) -> Result<()> {
    let flag = if force { "-D" } else { "-d" };
    run_git_ok(dir, &["branch", flag, branch])?;
    Ok(())
}

pub fn git_has_remote(dir: &Path, remote: &str) -> bool {
    run_git_ok(dir, &["remote", "get-url", remote]).is_ok()
}

pub fn git_current_branch(dir: &Path) -> Result<String> {
    let out = run_git_ok(dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    Ok(out.trim().to_string())
}

pub fn git_has_any_commits(dir: &Path) -> bool {
    run_git_ok(dir, &["rev-parse", "HEAD"]).is_ok()
}

pub fn git_init_commit(dir: &Path) -> Result<()> {
    // Ensures there's at least one commit
    if git_has_any_commits(dir) {
        return Ok(());
    }
    run_git_ok(dir, &["commit", "--allow-empty", "-m", "initial commit", "--no-verify"])?;
    Ok(())
}

pub fn git_list_conflicted_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let out = run_git_ok(dir, &["diff", "--name-only", "--diff-filter=U"])?;
    let files = out
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| dir.join(l))
        .collect();
    Ok(files)
}

pub(crate) fn git_ensure_user(dir: &Path) -> Result<()> {
    // In test environments we may need a user.email/user.name configured
    let email = run_git_ok(dir, &["config", "user.email"]).unwrap_or_default();
    if email.trim().is_empty() {
        run_git_ok(dir, &["config", "user.email", "balls@example.local"])?;
    }
    let name = run_git_ok(dir, &["config", "user.name"]).unwrap_or_default();
    if name.trim().is_empty() {
        run_git_ok(dir, &["config", "user.name", "balls"])?;
    }
    Ok(())
}

