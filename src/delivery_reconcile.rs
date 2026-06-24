//! bl-22dd — sync the checkout that owns the integration branch after a
//! plumbing `update-ref` moved it underneath them.
//!
//! Landing a commit on a branch has four independent effects: write the
//! objects, move the ref, append a reflog entry, and update the index + working
//! tree of the checkout on that branch. The squash delivers via `commit-tree` +
//! `update-ref` from the project gitdir ([`crate::delivery_repo`]) — deliberate
//! plumbing, so it never disturbs the working tree of an unrelated checkout — but
//! that very plumbing SKIPS the fourth effect for the checkout that owns
//! `integration`. The ref advances while that checkout's index + working tree
//! stay at the pre-delivery tree, so `git status` there reports the whole
//! delivered diff as a phantom *staged* change (the user's primary checkout, the
//! one they work in). git refuses to merge/checkout a branch checked out in
//! another worktree, which is why the squash uses `update-ref` to bypass the
//! guard — and bypassing the guard also bypasses the working-tree update it
//! protects. [`Project::reconcile`] restores that fourth effect, separately and
//! idempotently, so the ref-flip stays the atomic BINDING commit point (§14) and
//! a crash between the two is a transient state the next run heals, not manual
//! cleanup.

use std::io;
use std::path::PathBuf;

use crate::delivery_repo::Project;

impl Project {
    /// Sync every checkout that owns `integration` to the ref, healing the
    /// bl-22dd phantom. Acts on a checkout ONLY when it sits exactly at the
    /// delivery's parent (`HEAD^`) in both index and working tree — the
    /// phantom's signature, and proof there is no real local edit to clobber:
    /// the guard `update-ref` bypassed, restored. A checkout carrying genuine
    /// work fails the gate and is left untouched (refuse on dirty, never
    /// clobber). `git restore --source=HEAD --staged --worktree` makes index +
    /// worktree match the moved ref WITHOUT a second ref move (no reflog noise)
    /// and preserves untracked files. Idempotent and self-healing: a checkout
    /// already at the ref has its index at `HEAD`, not `HEAD^`, so the gate skips
    /// it — a retried close, or a crash between the ref-flip and this sync,
    /// converges on the next run. NEVER touches a `work/<id>` checkout: those sit
    /// on their own branch, not `integration`, so [`Self::checkouts_on`] excludes
    /// them.
    pub(crate) fn reconcile(&self, integration: &str) -> io::Result<()> {
        for ck in self.checkouts_on(integration)? {
            // `diff --quiet`: working tree == index. `diff --cached --quiet
            // HEAD^`: index == the delivery's parent. Both true ⇔ the checkout
            // is pristine one commit behind the just-moved ref — the phantom.
            let phantom = Self::ok(&ck, &["diff", "--quiet"])?
                && Self::ok(&ck, &["diff", "--cached", "--quiet", "HEAD^"])?;
            if phantom {
                Self::run(&ck, &["restore", "--source=HEAD", "--staged", "--worktree", ":/"])?;
            }
        }
        Ok(())
    }

    /// The non-bare checkouts that currently have `branch` checked out, read
    /// from `git worktree list --porcelain`. Each worktree is a block led by a
    /// `worktree <path>` line; the one that owns `branch` carries a `branch
    /// refs/heads/<branch>` line. The bare root (a `bare` block, no `branch`
    /// line) and every worktree on another branch — the agents' `work/<id>`
    /// trees included — carry no matching line, so a caller acts only on the
    /// checkout(s) that own `branch`.
    fn checkouts_on(&self, branch: &str) -> io::Result<Vec<PathBuf>> {
        let want = format!("branch refs/heads/{branch}");
        let out = Self::run(&self.root, &["worktree", "list", "--porcelain"])?;
        let mut paths = Vec::new();
        let mut cur: Option<PathBuf> = None;
        for line in out.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                cur = Some(PathBuf::from(p));
            } else if line == want {
                if let Some(p) = cur.take() {
                    paths.push(p);
                }
            }
        }
        Ok(paths)
    }
}

#[cfg(test)]
#[path = "delivery_reconcile_tests.rs"]
mod tests;
