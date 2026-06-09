//! §11/§14 deferred branch cleanup — `prime.post` prunes settled `work/<id>`
//! branches.
//!
//! Close/unclaim teardown removes only the worktree DIRECTORY; the branch must
//! survive the op (§11: "re-creatable from the branch, so it is rollback-safe").
//! The reason is the §14 unwind: a close whose later hook fails (the tracker
//! push sorts last) rolls back through `close.pre`'s un-squash — at which point
//! the `work/<id>` branch is the ONLY copy of the diff. Deleting it inside the
//! op would strand a rolled-back close with the work nowhere: worktree gone,
//! squash undone, branch deleted — and the retry's deliver would silently
//! no-op on the absent branch. So branch deletion is DEFERRED, non-transactional
//! cleanup ("deleting `work/<id>` is deferred, non-transactional cleanup
//! (`prime`)", §11/§14), and `prime` — which runs outside any op, after
//! re-materializing the still-claimed set — is the cleanup site. Without it the
//! branch namespace grew monotonically with every delivered task (bl-292d: 52
//! had accumulated).

use std::io;

use crate::delivery::{marker, Repo};
use crate::delivery_repo::Project;

impl Project {
    /// Delete every local `work/<id>` branch that is settled on the
    /// integration branch (delivered, or carrying no commit beyond its fork —
    /// nothing is lost; the delivery squash IS the record, the branch a stale
    /// second copy). Committed-but-undelivered work SURVIVES (the bl-65e0
    /// unclaim contract: a later claim + close delivers it; discard is an
    /// explicit `git branch -D`). A checked-out branch survives too — `git
    /// branch -D` refuses it, and the delete is BEST-EFFORT precisely so a
    /// live claim's branch (this actor's, or another claimant's on this
    /// machine) never fails a prime. So is the whole prune: a project root
    /// that is no git repo yet (a pre-claim prime) has nothing to clean.
    /// Idempotent: a pruned branch simply no longer enumerates.
    pub fn prune(&self) -> io::Result<()> {
        let Ok(integration) = self.integration() else {
            return Ok(()); // no repo / no HEAD branch — nothing to prune
        };
        let refs = Self::run(&self.root, &["for-each-ref", "--format=%(refname:short)", "refs/heads/work/"])?;
        for branch in refs.lines() {
            let id = branch.strip_prefix("work/").unwrap_or(branch);
            if self.settled(branch, &integration, &marker(id))? {
                Self::ok(&self.root, &["branch", "-D", branch])?; // best-effort: refused while checked out
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "delivery_prune_tests.rs"]
mod tests;
