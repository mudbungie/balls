//! Candidate arithmetic for the speculator (bl-d0c2, design
//! docs/design/bl-24e7-speculative-merge-queue.md) — the git acts that make a
//! queue prefix buildable without creating anything that can leak.
//!
//! A candidate is computed, never stored: `git merge-tree --write-tree` yields
//! the merged TREE with no branch, no index and no worktree involved, and
//! [`commit_tree`] wraps it in an UNREFERENCED merge commit (parents: the
//! previous candidate and the sealed tip) purely so the next `merge-tree` in
//! the chain has a committish with the right ancestry. Nothing points at these
//! objects — they are `git gc` food by construction, which is the design's
//! cleanup section made structural.
//!
//! The one materialization is [`build_dir`]/[`remove_build_dir`]: a DETACHED
//! worktree that exists only for the duration of one gate run. The invariant a
//! consumer may assert between rounds is that `git worktree list` shows only
//! real claims.

use std::io;
use std::path::Path;

use crate::safegit;

/// The outcome of merging one sealed tip onto the chain so far.
#[derive(Debug, PartialEq, Eq)]
pub enum Merge {
    /// A clean merge — the resulting tree OID.
    Tree(String),
    /// The merge conflicts. The candidate (and every deeper one) is
    /// unbuildable; the ball falls back to fold-at-close, where the branch
    /// owner — the only one whose judgment belongs in the resolution —
    /// resolves it (settles the design's open question 2).
    Conflict,
}

/// `git merge-tree --write-tree base tip` — the real merge machinery (rename
/// detection included), no side effects at all. Exit 1 is ambiguous: a
/// CONFLICTED merge still writes the tree OID as its first stdout line, while
/// "not something we can merge" writes a message — only the former is a
/// conflict; everything else is an error with git's voice.
pub fn merge_tree(repo: &Path, base: &str, tip: &str) -> io::Result<Merge> {
    let mut cmd = safegit::at(repo);
    cmd.args(["merge-tree", "--write-tree", base, tip]);
    let out = cmd.output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout.lines().next().unwrap_or("").trim().to_string();
    let is_oid = first.len() == 40 && first.bytes().all(|b| b.is_ascii_hexdigit());
    match out.status.code() {
        Some(0) => Ok(Merge::Tree(first)),
        Some(1) if is_oid => Ok(Merge::Conflict),
        _ => Err(io::Error::other(format!(
            "git merge-tree: {} {}",
            stdout.trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))),
    }
}

/// Wrap a candidate tree in an unreferenced merge commit so the chain's next
/// merge sees real ancestry. Identity is pinned and mechanical, like the
/// queue's tags — these commits carry no authorship, only structure.
pub fn commit_tree(repo: &Path, tree: &str, parents: &[&str]) -> io::Result<String> {
    let mut cmd = safegit::at(repo);
    cmd.args(["-c", "user.name=bl-speculate", "-c", "user.email=speculate@balls"]);
    cmd.args(["commit-tree", tree, "-m", "speculative candidate (bl-24e7)"]);
    for p in parents {
        cmd.args(["-p", p]);
    }
    let out = cmd.output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(io::Error::other(format!(
            "git commit-tree: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Materialize a candidate commit as a detached worktree at `dir` for one
/// gate run. The caller MUST pair this with [`remove_build_dir`].
pub fn build_dir(repo: &Path, commit: &str, dir: &Path) -> io::Result<()> {
    let mut cmd = safegit::at(repo);
    cmd.args(["worktree", "add", "--detach"]).arg(dir).arg(commit);
    let out = cmd.output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "git worktree add: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Tear the build worktree down, force included — a gate leaves debris
/// (target/, caches) and the worktree must go regardless.
pub fn remove_build_dir(repo: &Path, dir: &Path) -> io::Result<()> {
    let mut cmd = safegit::at(repo);
    cmd.args(["worktree", "remove", "--force"]).arg(dir);
    let out = cmd.output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "git worktree remove: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(test)]
#[path = "speculate_candidate_tests.rs"]
mod speculate_candidate_tests;
