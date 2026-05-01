//! Squash-merge a work branch into main even when `store.root` is a
//! bare gitdir. Non-bare roots run the squash directly. Bare roots
//! cannot host working-tree-required commands (`git merge --squash`
//! refuses with "this operation must be run in a work tree"), so we
//! provision an ephemeral detached worktree at `<root>/.balls/local/
//! squash-<pid>`, do the squash there, and update `refs/heads/<main>`
//! from the bare gitdir afterward. See bl-56f4: bare repos with linked
//! `.balls-worktrees/` checkouts are a designed-for layout, but the
//! direct-squash code path silently broke them.

use crate::error::Result;
use crate::git;
use crate::store::Store;
use std::path::{Path, PathBuf};

/// Squash-merge `branch` into `store.root`'s configured main branch,
/// producing a single commit with `msg`. Returns the new commit's SHA,
/// or `None` when the squash produced no staged changes (a "no-code"
/// review — the caller decides whether to skip the commit and emit a
/// `no-code` state-branch marker).
pub fn squash_into_main(store: &Store, branch: &str, msg: &str) -> Result<Option<String>> {
    if !is_bare_repo(&store.root)? {
        return squash_in_place(&store.root, branch, msg);
    }
    squash_in_detached_worktree(store, branch, msg)
}

fn squash_in_place(dir: &Path, branch: &str, msg: &str) -> Result<Option<String>> {
    git::git_merge_squash(dir, branch)?;
    if git::has_staged_changes(dir)? {
        git::git_commit(dir, msg)?;
        Ok(Some(git::git_resolve_sha(dir, "HEAD")?))
    } else {
        Ok(None)
    }
}

fn squash_in_detached_worktree(
    store: &Store,
    branch: &str,
    msg: &str,
) -> Result<Option<String>> {
    let main = git::git_current_branch(&store.root)?;
    let tmp = squash_worktree_path(store);
    scrub_path(&store.root, &tmp);
    if let Some(parent) = tmp.parent() {
        std::fs::create_dir_all(parent)?;
    }
    worktree_add_detach(&store.root, &tmp, &main)?;
    let result = squash_in_place(&tmp, branch, msg);
    if let Ok(Some(sha)) = result.as_ref() {
        update_ref(&store.root, &format!("refs/heads/{main}"), sha)?;
    }
    scrub_path(&store.root, &tmp);
    result
}

/// Process-id-suffixed temp path so concurrent `bl review` invocations
/// in the same store cannot collide on the worktree directory.
fn squash_worktree_path(store: &Store) -> PathBuf {
    store.local_dir().join(format!("squash-{}", std::process::id()))
}

/// Best-effort cleanup: detach git's record of the worktree first
/// (otherwise a leftover entry in `worktrees/` blocks a re-add at the
/// same path), then remove the directory itself in case `git` left
/// admin files behind. Errors are intentionally swallowed — a stale
/// path will be cleaned by the next squash, and surfacing this error
/// to the caller would mask the real failure.
fn scrub_path(repo: &Path, path: &Path) {
    if path.exists() {
        let _ = git::git_worktree_remove(repo, path, true);
    }
    if path.exists() {
        let _ = std::fs::remove_dir_all(path);
    }
}

/// True when `dir`'s gitdir has `core.bare = true`. Bare gitdirs
/// reject working-tree commands; callers must route those ops through
/// a real worktree.
fn is_bare_repo(dir: &Path) -> Result<bool> {
    Ok(git::run_git_ok(dir, &["rev-parse", "--is-bare-repository"])?.trim() == "true")
}

/// `git worktree add --detach <path> <ref>`: create a worktree at
/// `path` with a detached HEAD pointing at `ref`'s tip. Detached so
/// the squash commit doesn't claim the main branch — we plumb the
/// resulting SHA into `refs/heads/<main>` separately.
fn worktree_add_detach(dir: &Path, path: &Path, refname: &str) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    git::run_git_ok(dir, &["worktree", "add", "--detach", &path_str, refname])?;
    Ok(())
}

/// `git update-ref <name> <sha>`: move a ref to the given SHA. Used
/// to fast-forward main from the bare gitdir after the detached
/// worktree produced the squash commit.
fn update_ref(dir: &Path, name: &str, sha: &str) -> Result<()> {
    git::run_git_ok(dir, &["update-ref", name, sha])?;
    Ok(())
}

#[cfg(test)]
#[path = "bare_squash_tests.rs"]
mod tests;
