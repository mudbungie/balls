//! §11/§14 deferred branch cleanup — `prime.post` prunes settled `work/<id>`
//! branches.
//!
//! Close/unclaim teardown removes only the worktree DIRECTORY; the branch must
//! survive the op (§11: "re-creatable from the branch, so it is rollback-safe").
//! The reason is converge-on-retry (§14): until the squash lands — a close can
//! abort before it (gate failure, fold conflict) — the `work/<id>` branch is
//! the ONLY copy of the diff, and the retry's deliver recomputes from it.
//! Deleting it inside the op would make a retried abort silently no-op on the
//! absent branch. So branch deletion is DEFERRED, non-transactional cleanup
//! ("deleting `work/<id>` is deferred, non-transactional cleanup (`prime`)",
//! §11/§14), and `prime` — which runs outside any op, after re-materializing
//! the still-claimed set — is the cleanup site. Without it the branch namespace
//! grew monotonically with every delivered task (bl-292d: 52 had accumulated).

use std::io;

use crate::delivery::{marker, Repo};
use crate::delivery_repo::Project;
use crate::delivery_standing::Standing;

impl Project {
    /// Delete every local `work/<id>` branch that is SETTLED on the
    /// integration branch (content-contained in its delivery, or carrying no
    /// commit beyond its fork — nothing is lost; the delivery squash IS the
    /// record, the branch a stale second copy). Committed-but-undelivered work
    /// SURVIVES — both the never-delivered branch and the diverged one carrying
    /// content beyond its delivery (the bl-65e0 unclaim contract: a later
    /// claim-and-close delivers it — or, diverged, aborts loudly; discard is an
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
            if matches!(self.standing(branch, &integration, &marker(id))?, Standing::Settled) {
                Self::ok(&self.root, &["branch", "-D", branch])?; // best-effort: refused while checked out
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "delivery_prune_tests.rs"]
mod tests;
