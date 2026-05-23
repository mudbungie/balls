//! Legacy `.balls/worktree` → `.balls/state-repo` migration helpers.
//!
//! A standalone pre-bl-8a9a repo carries `balls/tasks` in its own git
//! with a `.balls/worktree` checkout. `state_repo::first_contact`
//! adopts the branch into the new state clone and retires the
//! worktree (SPEC-tracker-state §6); these helpers run on that path:
//!
//! - [`guard_legacy_worktree`] refuses the migration if the worktree
//!   carries uncommitted or untracked state — adoption is a fetch of
//!   the committed tip plus a forced worktree removal, so anything
//!   else would vanish silently (bl-c7b5).
//! - [`retire_legacy_worktree`] drops the linked worktree, prunes its
//!   registry entry, and deletes the now-adopted branch from the
//!   project git. Entirely best-effort once adoption succeeded.

use crate::error::{BallError, Result};
use crate::{git, git_state};
use std::fs;
use std::path::Path;

/// The legacy project-worktree checkout retired by the unified model.
pub(crate) const LEGACY_WORKTREE_REL: &str = ".balls/worktree";

/// Refuse migration if the legacy `.balls/worktree` carries uncommitted
/// or untracked state. Adoption fetches the committed `branch` tip then
/// force-removes the worktree, so anything off-branch is lost. On error
/// no `.balls/state-repo` exists yet — a retry after committing is a
/// clean first contact.
pub(crate) fn guard_legacy_worktree(root: &Path, branch: &str) -> Result<()> {
    let wt = root.join(LEGACY_WORKTREE_REL);
    if !wt.is_dir() {
        return Ok(());
    }
    let out = git::run_git_in(&wt, &["status", "--porcelain"])?;
    let dirty = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || dirty.is_empty() {
        return Ok(());
    }
    Err(BallError::Other(format!(
        "the legacy `.balls/worktree` at {wt} carries uncommitted changes:\n{dirty}\n\
         Migration to `.balls/state-repo` only adopts the committed tip of `{branch}` — \
         retiring the worktree would silently discard these edits. Commit them and re-run:\n  \
         (cd {wt} && git add -A && git commit -m 'preserve pre-migration state' --no-verify)",
        wt = wt.display(),
    )))
}

/// Retire the legacy `.balls/worktree`: drop the linked worktree, its
/// registry entry, and the project git's now-adopted `balls/tasks`
/// branch. Entirely best-effort — migration succeeds on the clone.
pub(crate) fn retire_legacy_worktree(root: &Path, branch: &str) {
    let wt = root.join(LEGACY_WORKTREE_REL);
    let _ = git::git_worktree_remove(root, &wt, true);
    let _ = git_state::worktree_prune(root);
    if wt.exists() {
        let _ = fs::remove_dir_all(&wt);
    }
    let _ = git::git_branch_delete(root, branch, true);
}

#[cfg(test)]
#[path = "state_repo_migrate_tests.rs"]
mod tests;
