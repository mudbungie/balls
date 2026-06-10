//! §12/§13 remote ops: `sync` (import) and `push` (publish). `sync` imports on
//! the explicit `bl sync` (and inside prime); `push` publishes after every
//! mutating op. Currency is OPTIMISTIC (mutate → push, bl-336a): there is no
//! pre-pull — a stale store surfaces atomically as the push's non-ff reject
//! (E5), and recovery is `bl sync` + retry. Both are no-ops in a stealth
//! (no-remote) repo: with no remote there is nothing to talk to, which is the
//! structural opt-out (§12).

use super::git::git;
use super::payload::Binding;
use crate::safegit::reject_option_like;
use std::io;
use std::path::Path;

/// §13 `sync/pre`: the general rule — fetch the branch's UPSTREAM, **if any**,
/// then **fast-forward** THAT branch. "If any" is read from the remote
/// ([`remote_has_branch`], the same ls-remote that decides prime's
/// adopt-vs-found): an upstream-less branch — the landing by construction (§4),
/// any local-only branch — yields a no-op *for free*, no name special-cased.
/// The ff target is the branch the binding NAMES, never whatever the store
/// checkout happens to have checked out: the store's own branch integrates by
/// `merge --ff-only FETCH_HEAD` (the working tree moves with it); any other
/// branch is a pure ref move via the `<branch>:<branch>` refspec (ff-only by
/// git's own default). Either way the ff is atomically detect-and-act — a
/// non-ff IS the contention signal (git's non-zero exit becomes ours: "remote
/// wins, re-run"), so there is no separate contention probe. Nothing is pushed,
/// so a partial sync leaves the branch at the old or the new tip, never wedged
/// (§13 rollback).
pub fn sync(b: &Binding) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    let store = Path::new(&b.store);
    let branch = b.tasks_branch.as_str();
    reject_option_like(remote)?;
    reject_option_like(branch)?;
    if !remote_has_branch(store, remote, branch)? {
        return Ok(()); // no upstream — the §13 no-op, for free
    }
    if git(store, &["symbolic-ref", "--short", "HEAD"]).ok().as_deref() == Some(branch) {
        git(store, &["fetch", remote, branch])?;
        if let Err(e) = git(store, &["merge", "--ff-only", "FETCH_HEAD"]) {
            return not_yet_cut_over(store, remote, branch).then_some(()).ok_or(e);
        }
    } else {
        git(store, &["fetch", remote, &format!("{branch}:{branch}")])?;
    }
    Ok(())
}

/// Is `remote`'s `branch` tip NOT a store — no `tasks/` tree at its root? That
/// is the §16 migration window (bl-868d): a hub still carrying the
/// PRE-greenfield legacy JSON store on the (colliding, §16) store-branch name,
/// awaiting the runbook's one-time human cutover. Such a tip is no upstream at
/// all — every store commit carries `tasks/` by construction (§2, the founding
/// `.gitkeep`) — so a failed integrate/publish against it is the window, not
/// contention: warn (the §12 diagnostic-never-authority pattern) and report
/// `true` so the caller skips, keeping work local and the legacy ref intact
/// (cutover is the runbook's explicit force-push, never an implicit rewrite).
/// Identification must be POSITIVE: the tip is re-fetched here (`FETCH_HEAD`),
/// and any failure to read it reports `false` — the caller's own error stands.
pub(super) fn not_yet_cut_over(repo: &Path, remote: &str, branch: &str) -> bool {
    if git(repo, &["fetch", remote, branch]).is_err()
        || git(repo, &["cat-file", "-e", "FETCH_HEAD:tasks"]).is_ok()
    {
        return false;
    }
    eprintln!("tracker: `{remote}`'s `{branch}` is not a greenfield store (its tip has no tasks/) — a legacy store awaiting cutover, left intact; this checkout's store stays local until the ref is cut over (docs/migration-runbook.md)");
    true
}

/// Does `remote` already carry `branch`? `git ls-remote --heads` is the one
/// round-trip that answers "an upstream, if any" — sync's no-op gate and
/// prime's adopt-vs-found / clone-vs-bootstrap signal (§12/§13).
pub(super) fn remote_has_branch(cwd: &Path, remote: &str, branch: &str) -> io::Result<bool> {
    Ok(!git(cwd, &["ls-remote", "--heads", remote, branch])?.is_empty())
}

/// §12 `*/post`: publish the just-sealed balls branch to the remote — always to
/// an ESTABLISHED store (founding is `prime`'s alone, §12). A rejected push
/// (non-ff, perms revoked mid-life, a server-hook reject) means the mutation did
/// NOT land while the caller believes it is federated, so the non-zero exit
/// ABORTS the op (the push IS the optimistic mutate → push contention check;
/// re-run after `bl sync`) — it is NEVER silently degraded to stealth, which
/// would be split-brain (contrast `prime`'s founding-miss fallback, where
/// nothing existed to land on). ONE carve-out, positively identified: a reject
/// against a remote tip that is NOT a store ([`not_yet_cut_over`], bl-868d) is
/// the §16 migration window — warn and keep the work local; the legacy ref is
/// never rewritten (cutover is the runbook's explicit force-push).
pub fn push(b: &Binding) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    reject_option_like(remote)?;
    reject_option_like(&b.tasks_branch)?;
    let store = Path::new(&b.store);
    if let Err(e) = git(store, &["push", remote, &b.tasks_branch]) {
        return not_yet_cut_over(store, remote, &b.tasks_branch).then_some(()).ok_or(e);
    }
    Ok(())
}

/// §6/§13 `install/pre`: fetch the center's config branch (`balls/config`,
/// [`crate::LANDING_BRANCH`]) into the LANDING repo so core can MATERIALIZE it
/// locally and copy it in. The tracker is balls' only remote-talker — core never
/// fetches (§0) — so `prime --install`'s remote read rides this hook. It leaves
/// the config at the landing's `FETCH_HEAD` (a git-standard ref, so no invented
/// core↔plugin convention); core reads it from the same checkout. This is a READ
/// only — config adoption is destructive on the LANDING, never a push to the
/// center (publishing is `install --to`, a separate direction). Stealth (no
/// remote) is a no-op, like every handler — and so is a present remote that
/// simply LACKS the ref (bl-45fd): bl never publishes the landing (§4
/// single-owner), so a stock hub carries no `balls/config`, and a purely local
/// install must not depend on remote state. The gate is sync's own
/// [`remote_has_branch`] ("an upstream, if any", §13); an adopt that really
/// needs the center's config fails at point-of-use (no `FETCH_HEAD`).
pub fn fetch_config(b: &Binding) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    reject_option_like(remote)?;
    let landing = Path::new(&b.landing);
    if !remote_has_branch(landing, remote, crate::LANDING_BRANCH)? {
        return Ok(()); // no landing on the hub — the §13 no-op, for free
    }
    git(landing, &["fetch", remote, crate::LANDING_BRANCH])?;
    Ok(())
}

#[cfg(test)]
#[path = "remote_ops_tests.rs"]
mod tests;
