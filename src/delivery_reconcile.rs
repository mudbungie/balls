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
//!
//! It restores it the way `git checkout` itself does: the two-way checkout
//! merge `git read-tree -m -u <old> <tip>` (bl-b69e). That updates exactly the
//! files the delivery changed, carries every other local edit forward, and
//! REFUSES atomically — touching nothing — when a delivered file is also edited
//! locally. An earlier gate (bl-22dd) acted only on a checkout pristine at
//! `HEAD^`, so one unrelated edit anywhere left the whole delivery showing as a
//! phantom staged revert; git's own gate is finer and just as safe.

use std::io;
use std::path::{Path, PathBuf};

use crate::delivery_repo::Project;

impl Project {
    /// Carry every checkout that owns `integration` (the ref this delivery
    /// moved) forward to it, healing the bl-22dd phantom; a checkout that
    /// cannot be carried safely is left untouched with one `bl-delivery:` line
    /// on stderr. NEVER fails the close over a checkout's state. Acts only on
    /// checkouts OF that ref — the root on `main` for a flat close, the epic's
    /// worktree for a nested one (bl-7b71); the closing ball's own `work/<id>`
    /// and every sibling's sit on their own branches, so [`Self::checkouts_on`]
    /// excludes them.
    pub(crate) fn reconcile(&self, integration: &str) -> io::Result<()> {
        for ck in self.checkouts_on(integration)? {
            if let Some(warning) = Self::carry_forward(&ck, integration)? {
                eprintln!("bl-delivery: {warning}");
            }
        }
        Ok(())
    }

    /// One checkout on `branch`, step by step; `Some` is the warning to print.
    ///
    /// 1. Refresh stat info, so a restatted-but-unchanged file is not "not
    ///    uptodate", then take the INDEX tree (`write-tree`). The index is the
    ///    true record of where the checkout stands. Unmerged entries make
    ///    `write-tree` fail: a conflict in progress is the human's, skip.
    /// 2. Index tree == the tip's tree ⇒ already current (a retried close, a
    ///    crash healed by the last run, an unrelated worktree edit): skip.
    ///    This is the idempotence.
    /// 3. `<old>` = the newest reflog entry of `branch` whose tree IS the index
    ///    tree. Not `HEAD^`: after an empty close or two unsynced deliveries it
    ///    names the wrong commit. Not the index tree itself: that would revert
    ///    staged edits. No match ⇒ staged changes sit on the checkout and no
    ///    `<old>` can be named safely: leave it, say so.
    /// 4. `read-tree -m -u <old> <tip>`. On refusal git changed nothing: leave
    ///    it, and name the exact command to run once the edit is moved aside.
    fn carry_forward(ck: &Path, branch: &str) -> io::Result<Option<String>> {
        let at = ck.display();
        let tip = Self::run(ck, &["rev-parse", &format!("refs/heads/{branch}")])?.trim().to_string();
        Self::ok(ck, &["update-index", "-q", "--refresh"])?;
        let Ok(index) = Self::run(ck, &["write-tree"]) else { return Ok(None) };
        let index = index.trim();
        if Self::run(ck, &["rev-parse", &format!("{tip}^{{tree}}")])?.trim() == index {
            return Ok(None);
        }
        // A branch with no reflog at all fails `log -g`: same as no match.
        let log = Self::run(ck, &["log", "-g", "--format=%H %T", &format!("refs/heads/{branch}")]);
        let suffix = format!(" {index}");
        let log = log.unwrap_or_default();
        let Some(old) = log.lines().find_map(|l| l.strip_suffix(&suffix)) else {
            return Ok(Some(format!(
                "{at} is not at {branch}'s tip, but its index matches no recorded {branch} commit \
                 (staged changes?), so it was left alone; check `git -C {at} status` and bring it forward by hand"
            )));
        };
        if Self::ok(ck, &["read-tree", "-m", "-u", old, &tip])? {
            return Ok(None);
        }
        Ok(Some(format!(
            "{at} was NOT carried forward to {branch}: a file the close changed is also modified there, \
             and nothing was touched; move those edits aside, then run `git -C {at} read-tree -m -u {old} {tip}`"
        )))
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
