//! §11 fold rigor (bl-a04a): the strict-fold guard and the no-resurrection
//! invariant.
//!
//! bl-33db shipped a resurrection THROUGH the delivery gate: its close-fold hit
//! a modify/delete conflict against a sibling's delivered deletion, the
//! conflict got resolved work-side, and the squash silently re-landed the
//! deleted file on main. Two structural answers, both pure plumbing over
//! existing refs (derive-don't-store, zero new state):
//!
//! - **Strict fold** ([`ensure_no_merge_in_progress`]): the fold is git's
//!   DEFAULT merge — no `-X`/strategy side-picking ever rides it — and
//!   delivery NEVER concludes a half-merge it finds in the worktree. Capture's
//!   `add -A` + commit over a `MERGE_HEAD` would conclude the merge, silently
//!   resolving every modify/delete work-side (and committing conflict markers
//!   besides) — the resurrection's open door. Resolving is the AGENT's job;
//!   their resolution merge commit is ordinary work on `work/<id>`.
//! - **No-resurrection invariant** ([`ensure_no_resurrection`]): at squash,
//!   the squash's changed paths (diff vs the integration tip) must be a subset
//!   of the paths authored on `work/<id>` since its fork — every non-merge
//!   commit's changed paths plus each fold merge commit's resolution paths
//!   (its combined `--cc` diff; a fold-conflict resolution IS a work commit,
//!   so it counts). An excess path means the squash carries something the task
//!   never wrote — a resurrection or a leak — and aborts the close NAMING it.

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use crate::delivery_repo::Project;

/// The strict-fold guard: refuse to act in a worktree whose merge is still in
/// progress (`MERGE_HEAD` exists). Runs BEFORE capture — `add -A` + commit
/// there would conclude the half-merge with a silent work-side resolution of
/// every modify/delete (the bl-33db resurrection). The agent resolves and
/// commits the merge themselves, then retries the close.
pub(crate) fn ensure_no_merge_in_progress(path: &Path) -> io::Result<()> {
    if Project::ok(path, &["rev-parse", "--verify", "--quiet", "MERGE_HEAD"])? {
        return Err(io::Error::other(
            "a merge is in progress in the work worktree; delivery never concludes a half-merge \
             (capture would silently resolve every conflict work-side — the bl-33db resurrection). \
             Resolve the conflicts, commit the merge yourself, then retry the close",
        ));
    }
    Ok(())
}

/// The no-resurrection invariant, checked at squash time: every path the
/// squash would change (vs the integration tip) must have been authored on the
/// work branch since its fork. Authored = the union of `--name-only` paths
/// over the commits on `branch` not on `integration` — `--cc` so a fold merge
/// commit contributes exactly its resolution paths (where the result differs
/// from every parent), nothing main brought in. An excess path aborts the
/// close naming it.
pub(crate) fn ensure_no_resurrection(root: &Path, branch: &str, integration: &str) -> io::Result<()> {
    let squash = path_set(&Project::run(root, &["diff", "--name-only", integration, branch])?);
    let not_integration = format!("^{integration}");
    let authored = path_set(&Project::run(
        root,
        &["log", "--format=", "--name-only", "--cc", branch, &not_integration],
    )?);
    let excess: Vec<&str> = squash.difference(&authored).map(String::as_str).collect();
    if excess.is_empty() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "no-resurrection invariant: the squash of {branch} carries path(s) never authored on it \
         since its fork — a fold resolution resurrected or leaked them: {}",
        excess.join(", ")
    )))
}

/// One `--name-only` listing → a path set (blank lines dropped).
fn path_set(listing: &str) -> BTreeSet<String> {
    listing.lines().filter(|l| !l.is_empty()).map(str::to_string).collect()
}

#[cfg(test)]
#[path = "delivery_fold_tests.rs"]
mod tests;
