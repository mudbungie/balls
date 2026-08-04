//! §11 reconciliation rigor: the ancestry precondition (bl-a1a4), the
//! half-merge guard and the no-resurrection invariant (bl-a04a).
//!
//! The law is that RECONCILIATION IS THE SOURCE OWNER'S JOB. Delivery is a
//! validation and atomic-advance boundary, never a merge queue: for every
//! delivery edge `S -> T` it pins `P = tip(T)` once, requires `P` to ALREADY be
//! an ancestor of `tip(S)`, gates the exact source tree, and advances `T` from
//! `P` by CAS. Fractal by construction — child → `work/<parent>` and root →
//! integration are the same operation at every depth.
//!
//! bl-33db shipped a resurrection THROUGH the delivery gate: its close-fold hit
//! a modify/delete conflict against a sibling's delivered deletion, the
//! conflict got resolved work-side, and the squash silently re-landed the
//! deleted file on main. Three structural answers, all pure plumbing over
//! existing refs (derive-don't-store, zero new state):
//!
//! - **Ancestry precondition** ([`ensure_target_incorporated`]): a source that
//!   has not incorporated the pinned target tip is REFUSED before any merge,
//!   gate, squash or ref move. Delivery used to merge the target in itself, so
//!   a conflict it could not resolve was the only thing that stopped it — and
//!   a conflict it COULD resolve landed a tree no human had ever run. Now the
//!   only merge is the closer's own, in their own worktree, tested there.
//! - **Half-merge guard** ([`ensure_no_merge_in_progress`]): delivery NEVER
//!   concludes a half-merge it finds in the worktree. Capture's
//!   `add -A` + commit over a `MERGE_HEAD` would conclude the merge, silently
//!   resolving every modify/delete work-side (and committing conflict markers
//!   besides) — the resurrection's open door. Resolving is the AGENT's job;
//!   their resolution merge commit is ordinary work on `work/<id>`.
//! - **No-resurrection invariant** ([`ensure_no_resurrection`]): at squash,
//!   the squash's changed paths (diff vs the pinned fold base, bl-8b89) must be a subset
//!   of the paths authored on `work/<id>` since its fork — every non-merge
//!   commit's changed paths plus each reconciling merge commit's resolution
//!   paths (its combined `--cc` diff; a conflict resolution IS a work commit,
//!   so it counts). An excess path means the squash carries something the task
//!   never wrote — a resurrection or a leak — and aborts the close NAMING it.

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use crate::delivery_repo::Project;

/// The ANCESTRY PRECONDITION (bl-a1a4): the pinned target tip `base` must
/// ALREADY be an ancestor of `branch` before delivery does anything. It is the
/// whole of "the source owner reconciles" — one `merge-base --is-ancestor`
/// over two refs the delivery already holds, no state, no lease, no queue.
///
/// Delivery used to merge `base` into the work branch itself, which made close
/// a merge queue: a clean target advance was folded in automatically and the
/// gate then ran on a tree that had never existed anywhere a human could test
/// it, while a conflicting one was the only advance that stopped the close.
/// Both are the same mistake — the source owner is the only party that can
/// decide what incorporating the target MEANS. So a stale source refuses, and
/// the refusal names S, T and P and prescribes the remedy: merge or rebase the
/// target into the source worktree, resolve and test THERE, then retry.
///
/// `target` is the branch NAME the operator thinks in; `base` is the pinned SHA
/// the delivery actually acts on (bl-9522/bl-8b89 — one read serves as the
/// precondition's comparison point, the squash parent, the no-resurrection base
/// and the CAS old-value), so the voice carries both.
pub(crate) fn ensure_target_incorporated(root: &Path, branch: &str, target: &str, base: &str) -> io::Result<()> {
    if Project::ok(root, &["merge-base", "--is-ancestor", base, branch])? {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "stale source: {target} (pinned at {base}) is not yet in {branch}, and delivery never \
         reconciles — it validates the tree you tested and advances {target} to it. Merge or \
         rebase {target} into the {branch} worktree, resolve and test there, then re-run `bl close`"
    )))
}

/// The half-merge guard: refuse to act in a worktree whose merge is still in
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
/// squash would change (vs `base`, the PINNED integration tip the gated source
/// tree already contains — bl-8b89) must have been authored on the work branch
/// since its fork. Authored = the union of `--name-only` paths over the
/// commits on `branch` not on `base` — `--cc` so the closer's own reconciling
/// merge commit contributes exactly its resolution paths (where the result
/// differs from every parent), nothing the target brought in. An excess path
/// aborts the close naming it. Comparing against the LIVE tip made a mid-gate sibling
/// landing read as excess (a false resurrection abort naming innocent paths);
/// against the pin, a mid-gate move is the delivery CAS's clean rejection and
/// excess means only a real resurrection or leak.
pub(crate) fn ensure_no_resurrection(root: &Path, branch: &str, base: &str) -> io::Result<()> {
    let squash = path_set(&Project::run(root, &["diff", "--name-only", base, branch])?);
    let not_base = format!("^{base}");
    let authored = path_set(&Project::run(
        root,
        &["log", "--format=", "--name-only", "--cc", branch, &not_base],
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
