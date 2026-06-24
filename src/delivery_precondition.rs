//! §11 delivery PRECONDITION (bl-4a88) — the one predicate, two surfacings.
//!
//! bl-delivery shells out to git against the invocation path (`Project.root`)
//! on every act, so it has an undeclared precondition: `root` must be a git
//! repository (bare or with a work tree). The non-git dir was discovering it
//! lazily, at the worst moment, in git's raw `fatal: not a git repository`
//! voice — and only on the first `claim.post` / `close.pre`, after tasks were
//! already filed and could never be retired.
//!
//! The fix makes that precondition explicit and early, off ONE predicate
//! ([`crate::delivery::Repo::is_git_repo`]), with two surfacings sharing one
//! message ([`precondition_unmet`]):
//!
//! - **claim.post / close.pre** — the forward acts that first touch git —
//!   abort cleanly in balls' voice ([`require_repo`]).
//! - **prime.post** — warns and no-ops (the binary edge checks the predicate
//!   directly, since prime warns at founding before any task is filed and must
//!   never refuse).
//!
//! Lives in the delivery capability, never in core prime (severability §1):
//! removing the delivery default deletes config, it must not teach prime a
//! "git repo or not" branch.

use std::io;

use crate::delivery::Repo;

/// The clean balls-voice message both surfacings speak — the abort on
/// claim.post / close.pre ([`require_repo`]) and the prime.post warning. It
/// names the condition (the invocation path is not a git repo, so the
/// `work/<id>` lifecycle is unusable) and BOTH drains, so the tracker is never
/// un-drainable: `git init` backs delivery with a code repo, or `bl conf
/// remove` detaches delivery for pure task tracking.
#[must_use]
pub fn precondition_unmet(invocation_path: &str) -> String {
    format!(
        "{invocation_path} is not a git repository, so the work/<id> delivery worktree \
         cannot be created — delivery needs a code repo at the invocation path. Run \
         `git init` here to enable delivery, or detach delivery for pure task tracking: \
         `bl conf remove claim.post bl-delivery` (likewise close.pre, close.post, \
         unclaim.post, prime.post)."
    )
}

/// The delivery precondition GATE: the forward acts that first shell out to git
/// against `root` — claim.post (materialize) and close.pre (deliver) — abort
/// cleanly here when `root` is not a git repo, instead of letting git's raw
/// `fatal: not a git repository` surface. Every other op-phase (the
/// path-guarded teardowns, the rollbacks, reads) is ungated; prime surfaces the
/// same precondition as a non-aborting warning, so it does not route here.
pub fn require_repo(op: &str, phase: &str, rolling_back: bool, repo: &dyn Repo, invocation_path: &str) -> io::Result<()> {
    let gated = !rolling_back && matches!((op, phase), ("claim", "post") | ("close", "pre"));
    if gated && !repo.is_git_repo()? {
        return Err(io::Error::other(precondition_unmet(invocation_path)));
    }
    Ok(())
}

#[cfg(test)]
#[path = "delivery_precondition_tests.rs"]
mod tests;
