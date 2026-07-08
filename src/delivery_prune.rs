//! §11/§14 deferred branch cleanup — `prime.post` prunes settled `work/<id>`
//! branches and, beside that prune (bl-c117, piece 3 of
//! docs/design/bl-18bf-prime-convergence.md), REPORTS the debris an unsettled
//! one leaves when its worktree directory is gone.
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
//!
//! An unsettled branch (committed-but-undelivered, or diverged past a delivery)
//! is correctly never pruned — but when its worktree directory is ALSO absent
//! (release/unclaim tore it down, or a human `rm -rf`'d it), that is silent
//! debris: content sitting on a branch nobody can see without already knowing
//! its name. bl-18bf's attack record explicitly BROKE the wider "claimed ball,
//! missing worktree" variant of this report — the §7 prime payload carries no
//! claim set, and the worktree is plugin territory core cannot stat — so this
//! report is deliberately narrow: branch present, dir absent, computed by the
//! plugin alone from state it already reads for the prune. Zero new subprocess
//! spawns: the same `for-each-ref` enumeration and [`Project::standing`] call
//! serve both the delete and the report; the only addition is one `exists()`
//! per unsettled branch.

use std::io;

use crate::delivery::Repo;
use crate::delivery_path::{marker, worktree_path};
use crate::delivery_repo::Project;
use crate::delivery_standing::Standing;
use crate::layout::Xdg;

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
    ///
    /// Every SURVIVING branch is then checked for debris (bl-c117): if its
    /// worktree — the same `(xdg, plugin, invocation, id)` formula `claim`
    /// derives ([`worktree_path`]) — is absent, one report line naming both
    /// remedies is RETURNED (never printed here; the plugin binary owns
    /// stderr, matching every other bl-b1be-style report in this codebase).
    /// `xdg`/`plugin` are exactly the binding inputs `claim` resolves its own
    /// worktree from — the caller already has them for that reason.
    pub fn prune(&self, xdg: &Xdg, plugin: &str) -> io::Result<Vec<String>> {
        let Ok(integration) = self.integration() else {
            return Ok(Vec::new()); // no repo / no HEAD branch — nothing to prune or report
        };
        let invocation = self.root.to_string_lossy().into_owned();
        let refs = Self::run(&self.root, &["for-each-ref", "--format=%(refname:short)", "refs/heads/work/"])?;
        let mut reports = Vec::new();
        for branch in refs.lines() {
            let id = branch.strip_prefix("work/").unwrap_or(branch);
            match self.standing(branch, &integration, &marker(id))? {
                Standing::Settled => {
                    Self::ok(&self.root, &["branch", "-D", branch])?; // best-effort: refused while checked out
                }
                Standing::Undelivered | Standing::Diverged => {
                    if !worktree_path(xdg, plugin, &invocation, id).exists() {
                        reports.push(debris_report(id, branch));
                    }
                }
            }
        }
        Ok(reports)
    }
}

/// The bl-c117 debris line: `branch` (`work/<id>`) is committed but its
/// worktree is gone. Names both remedies the design record specifies — re-claim
/// (the bl-65e0 contract: a later claim-and-close still delivers it) or
/// explicit discard — and prunes neither.
fn debris_report(id: &str, branch: &str) -> String {
    format!(
        "{branch} is committed but its worktree is gone — bl claim {id} \
         re-materializes onto it (a later close still delivers, bl-65e0), \
         or discard with git branch -D {branch}"
    )
}

#[cfg(test)]
#[path = "delivery_prune_tests.rs"]
mod tests;
