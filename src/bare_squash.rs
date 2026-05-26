//! Squash-merge a work branch into the integration branch by writing
//! the commit with plumbing (`commit-tree` + `update-ref`) instead of
//! porcelain (`git merge --squash` + `git commit`). The porcelain path
//! needs a work tree; plumbing doesn't — so the same code path covers
//! bare gitdirs and non-bare roots, and the case where the integration
//! branch isn't the one checked out at `store.root`. Bit-identical
//! commit object versus the old porcelain path (same tree, same parent,
//! same message) so observers downstream of the squash see no change.
//!
//! Pre-bl-cb73 this module provisioned an ephemeral detached worktree
//! under `<root>/.balls/local/squash-<pid>/` to host `git merge --squash`
//! whenever the in-place porcelain path didn't apply. That mechanism
//! produced the same commit a `commit-tree` over the task worktree's
//! post-merge tree produces directly — `review::review_worktree` has
//! already merged the integration branch into the task worktree, so
//! `<branch>^{tree}` IS the tree the squash commit should ship.
//!
//! `is_bare_repo` stays here because it shares the bareness-as-routing
//! shape — callers in `store_paths`, `store_init`, and the legacy-plugin
//! migration still need to detect a bare root.

use crate::error::Result;
use crate::git;
use crate::store::Store;
use std::path::Path;

/// Squash-merge `branch` into `store.root`'s `main_branch`, producing
/// a single commit with `msg`. Returns the new commit's SHA, or `None`
/// when the squash produced no tree change (a "no-code" review — the
/// caller decides whether to skip the commit and emit a `no-code`
/// state-branch marker).
///
/// Implementation: `review::review_worktree` has already merged
/// `main_branch` into the task worktree's `branch`, so `branch^{tree}`
/// is the post-merge tree. Compare it to `main_branch^{tree}`; equal
/// trees ⇒ no-code. Otherwise `commit-tree <tree> -p <main-tip>` makes
/// the squash commit, `update-ref` installs it at `refs/heads/<main>`,
/// and — only when `main_branch` is the branch checked out at a
/// non-bare `store.root` — a final `reset --hard HEAD` re-syncs the
/// user's work tree to the moved branch.
pub fn squash_into_main(
    store: &Store,
    branch: &str,
    msg: &str,
    main_branch: &str,
) -> Result<Option<String>> {
    let merged_tree = git::git_resolve_sha(&store.root, &format!("{branch}^{{tree}}"))?;
    let main_tree = git::git_resolve_sha(&store.root, &format!("{main_branch}^{{tree}}"))?;
    if merged_tree == main_tree {
        return Ok(None);
    }
    let main_tip = git::git_resolve_sha(&store.root, main_branch)?;
    let new_sha = git::git_commit_tree(&store.root, &merged_tree, &[&main_tip], msg)?;
    git::git_update_ref(&store.root, &format!("refs/heads/{main_branch}"), &new_sha)?;
    if integration_branch_is_checked_out(&store.root, main_branch)? {
        git::git_reset_hard(&store.root, "HEAD")?;
    }
    Ok(Some(new_sha))
}

/// True when `main_branch` is the branch checked out at `root` and
/// `root` is non-bare — i.e. `update-ref` on the integration branch
/// just moved the ref out from under a live work tree. The caller
/// follows up with `reset --hard HEAD` so the work tree mirrors the
/// new tip (preserves the pre-cb73 user-visible behavior: after `bl
/// review`, `git status` at the root shows clean against the squash).
/// Shared with `review_safety::rewind_main` so rewind and squash
/// agree on which case needs the re-sync.
pub(crate) fn integration_branch_is_checked_out(root: &Path, main_branch: &str) -> Result<bool> {
    if is_bare_repo(root)? {
        return Ok(false);
    }
    Ok(git::git_current_branch(root)? == main_branch)
}

/// True when `dir`'s gitdir has `core.bare = true`. Bare gitdirs
/// reject working-tree commands; the squash path doesn't care anymore
/// (it's pure plumbing now), but other callers — `store_paths`,
/// `store_init`, `legacy_plugin_migrate` — still branch on bareness
/// for discovery and migration concerns.
pub fn is_bare_repo(dir: &Path) -> Result<bool> {
    Ok(git::run_git_ok(dir, &["rev-parse", "--is-bare-repository"])?.trim() == "true")
}

#[cfg(test)]
#[path = "bare_squash_tests.rs"]
mod tests;
