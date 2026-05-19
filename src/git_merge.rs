//! Merge and conflict helpers split out of `git.rs`. These are the
//! git operations that classify a merge's outcome (clean / up-to-date
//! / conflict) and inspect in-progress conflict state — a cohesive
//! slice of the otherwise-flat git wrapper surface. Re-exported from
//! `git` so call sites keep using `git::git_merge` etc.

use crate::error::{BallError, Result};
use crate::git::{run_git_in, run_git_ok};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum MergeResult {
    Clean,
    UpToDate,
    Conflict,
}

pub fn git_merge(dir: &Path, branch: &str) -> Result<MergeResult> {
    classify_merge(run_git_in(dir, &["merge", "--no-edit", branch])?, "merge")
}

/// Squash merge: stage all changes from branch but do NOT commit.
/// Caller must call `git_commit()` afterward to finalize.
pub fn git_merge_squash(dir: &Path, branch: &str) -> Result<MergeResult> {
    classify_merge(
        run_git_in(dir, &["merge", "--squash", branch])?,
        "merge --squash",
    )
}

fn classify_merge(out: std::process::Output, what: &str) -> Result<MergeResult> {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        if stdout.contains("Already up to date") || stdout.contains("Already up-to-date") {
            return Ok(MergeResult::UpToDate);
        }
        return Ok(MergeResult::Clean);
    }
    let combined = format!("{stdout}{stderr}");
    if combined.contains("CONFLICT") || combined.contains("conflict") {
        Ok(MergeResult::Conflict)
    } else {
        Err(BallError::Git(format!("{} failed: {}", what, combined.trim())))
    }
}

pub fn is_merging(dir: &Path) -> bool {
    // Linked worktrees keep MERGE_HEAD in the per-worktree admin dir
    // (`.git/worktrees/<name>/`), not the shared common dir.
    // `--git-dir` returns the per-worktree path.
    run_git_ok(dir, &["rev-parse", "--git-dir"]).is_ok_and(|s| {
        let gd = PathBuf::from(s.trim());
        let abs = if gd.is_absolute() { gd } else { dir.join(gd) };
        abs.join("MERGE_HEAD").exists()
    })
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
