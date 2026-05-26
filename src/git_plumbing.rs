//! Plumbing helpers split out of `git.rs`. `commit-tree` synthesizes a
//! commit object without touching the index or a work tree;
//! `update-ref` installs a ref at a SHA. Pair them to land a squash
//! commit from a bare gitdir (bl-cb73) — no detached worktree needed.
//! Re-exported through `git::*` so call sites keep their imports flat.

use crate::error::Result;
use crate::git::run_git_ok;
use std::path::Path;

/// Write a commit object with the given tree, parents, and message;
/// return the new commit's SHA. Side-effect-free with respect to refs
/// and the working tree.
pub fn git_commit_tree(dir: &Path, tree_sha: &str, parents: &[&str], msg: &str) -> Result<String> {
    let mut args = vec!["commit-tree", tree_sha];
    for p in parents {
        args.push("-p");
        args.push(p);
    }
    args.push("-m");
    args.push(msg);
    Ok(run_git_ok(dir, &args)?.trim().to_string())
}

/// Set a ref to a SHA. Pair with `git_commit_tree` to install a
/// synthesized commit at a branch tip from a bare gitdir.
pub fn git_update_ref(dir: &Path, refname: &str, sha: &str) -> Result<()> {
    run_git_ok(dir, &["update-ref", refname, sha])?;
    Ok(())
}

#[cfg(test)]
#[path = "git_plumbing_tests.rs"]
mod tests;
