//! §11/§14 deferred branch cleanup — `prime.post` prunes settled `work/<id>`
//! branches and, beside that prune (bl-c117, piece 3 of
//! docs/design/bl-18bf-prime-convergence.md), REPORTS the debris an unsettled
//! one leaves when its worktree directory is gone.
//!
//! Close/unclaim teardown removes only the worktree DIRECTORY; the branch must
//! survive the op (§11: "re-creatable from the branch, so it is rollback-safe").
//! The reason is converge-on-retry (§14): until the squash lands — a close can
//! abort before it (gate failure, stale-source refusal) — the `work/<id>` branch is
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
//!
//! The report has TWO ARMS, decided by the store (bl-baa0). A branch outlives
//! its ball: close deletes `tasks/<id>.md` (§10 — absence IS the record) and
//! leaves the branch for this deferred cleanup, so the debris of a CLOSED ball
//! is the common case, and the re-claim remedy is a lie there — the ball cannot
//! be claimed and no close can deliver it. Deciding costs one more `exists()`
//! on the store checkout, which for `prime.post` is simply the plugin's CWD
//! (§13 diffless — the same cwd `close.pre` recovers its id from): no wire
//! widening, no claim set, no assertion about claim state — only "is there a
//! task file". This is narrower than the "claimed ball, missing worktree"
//! variant bl-18bf's attack record broke, and does not revive it.

use std::io;
use std::path::Path;

use crate::delivery::Repo;
use crate::delivery_path::{marker, worktree_path};
use crate::delivery_repo::Project;
use crate::delivery_standing::Standing;
use crate::layout::Xdg;
use crate::taskfile;

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
    /// derives ([`worktree_path`]) — is absent, one report line is RETURNED
    /// (never printed here; the plugin binary owns stderr, matching every other
    /// bl-b1be-style report in this codebase), naming the remedies that are
    /// actually open given whether `store` still holds the ball
    /// ([`Self::debris_report`]). `xdg`/`plugin` are exactly the binding inputs
    /// `claim` resolves its own worktree from, and `store` is the store
    /// checkout `prime.post` already runs in — the caller has all three for
    /// reasons that predate this report.
    pub fn prune(&self, xdg: &Xdg, plugin: &str, store: &Path) -> io::Result<Vec<String>> {
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
                        reports.push(self.debris_report(store, &integration, id, branch)?);
                    }
                }
            }
        }
        Ok(reports)
    }

    /// The bl-c117 debris line for `branch` (`work/<id>`): committed, worktree
    /// gone. Which remedies exist is the STORE's answer (bl-baa0), so the arm
    /// turns on `tasks/<id>.md` under the `store` checkout (§10 — absence IS
    /// the record):
    ///
    /// - **Open ball** — both remedies the design record specifies: re-claim
    ///   (the bl-65e0 contract, a later claim-and-close still delivers it) or
    ///   explicit discard.
    /// - **Closed ball** — re-claim is impossible and no close can deliver, so
    ///   ONLY discard is named. Because deletion is then the sole path, the
    ///   line first says whether anything would be lost: content-containment
    ///   against the integration TIP ([`Project::contained`]), not against a
    ///   `[bl-id]`-tagged delivery. That is the exact gap `Standing` cannot
    ///   see — content that landed inside ANOTHER ball's squash reads
    ///   `Undelivered` forever while being fully present on `integration`. One
    ///   `merge-tree` per closed-ball debris branch, off the clean path
    ///   entirely; a non-contained branch gets the three-dot diff to inspect
    ///   instead of a claim about its content.
    ///
    /// Prunes neither arm — reporting is the whole contract.
    fn debris_report(&self, store: &Path, integration: &str, id: &str, branch: &str) -> io::Result<String> {
        if taskfile::exists(store, id) {
            return Ok(format!(
                "{branch} is committed but its worktree is gone — bl claim {id} \
                 re-materializes onto it (a later close still delivers, bl-65e0), \
                 or discard with git branch -D {branch}"
            ));
        }
        let fate = if self.contained(branch, integration)? {
            format!("its content is already contained in {integration}, so discard it with git branch -D {branch}")
        } else {
            format!(
                "its content is NOT contained in {integration} — read it with \
                 git diff {integration}...{branch}, then discard with git branch -D {branch}"
            )
        };
        Ok(format!(
            "{branch} is committed but its worktree is gone, and {id} is closed \
             (no task file — absence is the record), so nothing can re-claim or \
             deliver it: {fate}"
        ))
    }
}

#[cfg(test)]
#[path = "delivery_prune_tests.rs"]
mod tests;
