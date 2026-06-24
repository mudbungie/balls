//! §11/§14 delivery standing — where `work/<id>` sits relative to this
//! incarnation's delivery (the converge-on-retry predicate, bl-430e/bl-c231).
//!
//! The squash is the delivery's BINDING commit point (§14): once a `[bl-id]`
//! commit stands on the integration branch, a retried close must DETECT it and
//! converge — never mint a duplicate. Detection is fork-scoped tag presence;
//! but presence alone cannot decide the retry, because ancestry cannot
//! distinguish "content included in the delivery" from "added after": a forge
//! squash-merge ALWAYS leaves `work/<id>`'s commits ancestry-unmerged while
//! their content landed (bl-7bfe), and a post-delivery commit (the bl-65e0
//! handoff: A's close squashed then aborted, B reclaimed and committed more)
//! looks identical to git's ancestry. The discriminator is CONTENT-CONTAINMENT
//! ([`Project::contained`]): a content-merge of the branch into the delivery
//! commit (`git merge-tree`) that reproduces the delivery tree means the branch
//! contributes nothing beyond it — skip; anything more means undelivered work a
//! silent skip would strand — the close must abort loudly instead.

use std::io;
use std::process::Stdio;

use crate::delivery_repo::Project;

/// Where `branch` stands relative to `integration` and this incarnation's
/// delivery — the one predicate `deliver`'s retry path and the prime prune
/// ([`Project::prune`]) both read through.
pub(crate) enum Standing {
    /// Nothing undelivered: fully merged, or a fork-scoped delivery commit
    /// CONTAINS the branch's content. Deliver skips; prune may delete.
    Settled,
    /// No delivery commit since the fork — the normal path: deliver proceeds.
    Undelivered,
    /// A delivery commit stands since the fork AND the branch carries content
    /// beyond it (the bl-65e0 handoff onto a delivered-but-unsealed close).
    /// Deliver must abort loudly; prune preserves the branch.
    Diverged,
}

impl Project {
    /// Classify `branch` (which must exist). Fully-merged (`--is-ancestor`) is
    /// settled outright; otherwise the `marker` scan is scoped to the fork so a
    /// reused id's PRIOR delivery, always an ancestor of the fork point, cannot
    /// false-positive (bl-430e/§11), and the newest delivery since the fork is
    /// tested for content-containment.
    pub(crate) fn standing(&self, branch: &str, integration: &str, marker: &str) -> io::Result<Standing> {
        if Self::ok(&self.root, &["merge-base", "--is-ancestor", branch, integration])? {
            return Ok(Standing::Settled);
        }
        let base = Self::run(&self.root, &["merge-base", integration, branch])?.trim().to_string();
        let since_fork = self.marked(&format!("{base}..{integration}"), marker)?;
        let Some(delivery) = since_fork.first() else {
            return Ok(Standing::Undelivered);
        };
        if self.contained(branch, delivery)? {
            Ok(Standing::Settled)
        } else {
            Ok(Standing::Diverged)
        }
    }

    /// Is `branch`'s content CONTAINED in the `delivery` commit — is a
    /// content-merge of the branch into it a no-op? `git merge-tree
    /// --write-tree` performs the real 3-way merge without touching any
    /// worktree; containment means it exits clean AND its merged tree IS the
    /// delivery's own tree. A conflict or any tree drift is non-containment —
    /// the branch carries something the delivery does not. This is what keeps
    /// the forge squash-merge a skip (same content, rewritten commits) while
    /// catching a genuinely undelivered commit.
    fn contained(&self, branch: &str, delivery: &str) -> io::Result<bool> {
        let out = Project::git(&self.root, &["merge-tree", "--write-tree", delivery, branch])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()?;
        let merged = String::from_utf8_lossy(&out.stdout);
        let merged = merged.lines().next().unwrap_or("");
        let tree = Self::run(&self.root, &["rev-parse", &format!("{delivery}^{{tree}}")])?;
        Ok(out.status.success() && merged == tree.trim())
    }
}

#[cfg(test)]
#[path = "delivery_standing_tests.rs"]
mod tests;
