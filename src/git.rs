use crate::error::{BallError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

// Merge/conflict helpers live in `git_merge` to keep this file under
// the 300-line cap. Re-exported so call sites keep using `git::*`.
pub use crate::git_merge::{
    git_list_conflicted_files, git_merge, git_merge_squash, is_merging, MergeResult,
};

/// Env vars git reads to locate its repo/index. We always pass the
/// repo via `current_dir`, so inherited values of these would bypass
/// our intent (e.g. inside a git hook). Scrub them on every spawn.
///
/// `pub` so the test harness scrubs the exact same set — see
/// `git_test_support` and `tests/common`.
pub const GIT_ENV_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_PREFIX",
];

pub fn clean_git_command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir);
    for var in GIT_ENV_VARS {
        cmd.env_remove(var);
    }
    cmd
}

pub(crate) fn run_git_in(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    let out = clean_git_command(dir)
        .args(args)
        .output()
        .map_err(|e| BallError::Git(format!("failed to spawn git: {e}")))?;
    Ok(out)
}

pub(crate) fn run_git_ok(dir: &Path, args: &[&str]) -> Result<String> {
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

/// `git rm -f`: remove from index and working tree, even if the file
/// has uncommitted modifications. Balls always uses -f because the
/// archiving path may have just mutated the task file's fields before
/// deleting it.
pub fn git_rm_force(dir: &Path, paths: &[&Path]) -> Result<()> {
    let mut args = vec!["rm", "-f", "--"];
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
    // Always commit mid-merge — we need both parents recorded even
    // when the index matches HEAD.
    if !has_staged_changes(dir)? && !is_merging(dir) {
        return Ok(());
    }
    run_git_ok(dir, &["commit", "-m", message, "--no-verify"])?;
    Ok(())
}

pub fn git_commit_empty(dir: &Path, msg: &str) -> Result<()> {
    run_git_ok(dir, &["commit", "--allow-empty", "-m", msg, "--no-verify"])?;
    Ok(())
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

pub fn git_push(dir: &Path, remote: &str, branch: &str) -> Result<()> {
    run_git_ok(dir, &["push", remote, branch])?;
    Ok(())
}

/// `git clone --bare <source> <gitdir>`. Runs from `gitdir`'s parent
/// (created if absent) so the bare gitdir lands at exactly that path,
/// e.g. `<workspace>/.git`. Used by the `bl init --bare` bootstrap.
pub fn git_clone_bare(source: &str, gitdir: &Path) -> Result<()> {
    let parent = gitdir.parent().unwrap_or(gitdir);
    std::fs::create_dir_all(parent)?;
    run_git_ok(parent, &["clone", "--bare", source, &gitdir.to_string_lossy()])?;
    Ok(())
}

/// `git config <key> <value>` in `dir`'s repo. Idempotent: re-setting
/// the same key overwrites with the same value.
pub fn git_config_set(dir: &Path, key: &str, value: &str) -> Result<()> {
    run_git_ok(dir, &["config", key, value])?;
    Ok(())
}

/// `git reset --hard <revspec>` — used by lifecycle rollback paths
/// to undo a transition's local commits when a required participant
/// rejected the negotiation.
pub fn git_reset_hard(dir: &Path, revspec: &str) -> Result<()> {
    run_git_ok(dir, &["reset", "--hard", revspec])?;
    Ok(())
}

/// Resolve a ref to its full SHA.
pub fn git_resolve_sha(dir: &Path, refname: &str) -> Result<String> {
    Ok(run_git_ok(dir, &["rev-parse", refname])?.trim().to_string())
}

/// Read the subject line of a commit. Returns `None` if the commit
/// doesn't exist in the object database.
pub fn git_commit_subject(dir: &Path, sha: &str) -> Option<String> {
    run_git_ok(dir, &["show", "-s", "--format=%s", sha])
        .ok()
        .map(|s| s.trim().to_string())
}

/// True if `sha` is an ancestor of `branch` (reachable from its tip).
pub fn git_is_ancestor(dir: &Path, sha: &str, branch: &str) -> bool {
    let out = clean_git_command(dir)
        .args(["merge-base", "--is-ancestor", sha, branch])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

/// Find the newest commit reachable from `branch` whose subject
/// contains `pattern` as a fixed string. Returns the full SHA, or
/// `None` if no such commit exists.
pub fn git_log_find_subject(dir: &Path, branch: &str, pattern: &str) -> Option<String> {
    let out = run_git_ok(
        dir,
        &["log", "-1", "--format=%H", "-F", "--grep", pattern, branch],
    )
    .ok()?;
    let sha = out.trim().to_string();
    if sha.is_empty() {
        None
    } else {
        Some(sha)
    }
}

/// Return the short form of a SHA via `git rev-parse --short`.
pub fn git_short_sha(dir: &Path, sha: &str) -> Option<String> {
    run_git_ok(dir, &["rev-parse", "--short", sha])
        .ok()
        .map(|s| s.trim().to_string())
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
    if git_has_any_commits(dir) {
        return Ok(());
    }
    run_git_ok(dir, &["commit", "--allow-empty", "-m", "initial commit", "--no-verify"])?;
    Ok(())
}

pub(crate) fn git_ensure_user(dir: &Path) -> Result<()> {
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

