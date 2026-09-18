//! §12/§13 remote ops: `sync` and `push`, both over ONE reconcile (bl-3616 §3,
//! bl-21ab). Currency is OPTIMISTIC (mutate → push, bl-336a): there is no
//! pre-pull — a stale store surfaces atomically as the push's non-ff reject,
//! and the reject is where the reconcile runs: fetch, `git rebase` the local
//! seals onto the remote tip IN the store checkout (never `update-ref` plumbing
//! behind it — the checkout IS the branch, §8/bl-057a), push once more. Every
//! seal is one commit touching one `tasks/<id>.md`, so seals on DIFFERENT balls
//! rebase clean by construction and the op lands with no human in the loop;
//! only a SAME-ball race conflicts, and that is E5 with the ball named. `sync`
//! is the same reconcile over every unpublished seal, the one place a store
//! publishes when the tracker is not wired on `*.post`. Transport failure
//! fails OPEN in both (warn; the store stays ahead, drift renders it); only a
//! same-ball conflict or a reject the reconcile cannot clear (permissions, a
//! server hook) stays fail-closed. Both are no-ops in a stealth (no-remote)
//! repo: with no remote there is nothing to talk to (§12).

use super::git::git;
use super::payload::Binding;
use super::Env;
use crate::safegit::reject_option_like;
use std::io;
use std::path::Path;

// The reconcile's spoken outcomes — the same-ball refusal and the fail-open
// warning — live in a sibling so this file stays under the 300-line cap.
#[path = "remote_voice.rs"]
mod voice;
use voice::{conflict, fail_open, unmerged_balls};

/// §13 `sync/pre`: the general rule — fetch the branch's UPSTREAM, **if any**,
/// then reconcile THAT branch. "If any" is read from the remote
/// ([`remote_has_branch`], the same ls-remote that decides prime's
/// adopt-vs-found): an upstream-less branch — the landing by construction (§4),
/// any local-only branch — yields a no-op *for free*, no name special-cased; an
/// unreachable remote fails OPEN ([`fail_open`]). The target is the branch the
/// binding NAMES, never whatever the store checkout happens to have checked
/// out: the store's own branch goes through [`reconcile`] (the working tree
/// moves with it, and its unpublished seals publish); any other branch is a
/// pure ref move via the `<branch>:<branch>` refspec (ff-only by git's own
/// default). A partial sync leaves the branch at the old or the new tip, never
/// wedged (§13 rollback) — a conflicted rebase is aborted before returning.
pub fn sync(b: &Binding, env: &Env) -> io::Result<()> {
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    let store = Path::new(&b.store);
    let branch = b.tasks_branch.as_str();
    reject_option_like(remote)?;
    reject_option_like(branch)?;
    match remote_has_branch(store, remote, branch) {
        Ok(false) => Ok(()), // no upstream — the §13 no-op, for free
        Err(e) => {
            fail_open(remote, &e);
            Ok(())
        }
        Ok(true) if git(store, &["symbolic-ref", "--short", "HEAD"]).ok().as_deref() == Some(branch) => {
            reconcile(b, env, remote)
        }
        // The refspec form's own non-ff is git's to spell: that one command is
        // fetch AND ref move, so its failure is ambiguous (unreachable remote,
        // absent ref, non-ff) and only the checked-out path is a positive loss.
        Ok(true) => git(store, &["fetch", remote, &format!("{branch}:{branch}")]).map(drop),
    }
}

/// §12 `*/post`: publish the just-sealed balls branch to the remote — always to
/// an ESTABLISHED store (founding is `prime`'s alone, §12). A rejected push is
/// the optimistic contention check firing, and it is answered by ONE
/// [`reconcile`] — the post-reject pull on the contended path only, so the
/// happy path still pays no round-trip (§12's "deliberately NO pre-pull"
/// stands). There is exactly one local seal on this path, the in-flight op's:
/// a clean rebase means the contention was on a DIFFERENT ball (the common
/// case with many agents) and the op lands; a conflict means the SAME ball
/// changed on both sides — the rebase is aborted, the non-zero exit ABORTS the
/// op (core un-seals), and the sentence names the ball. NEVER a silent stealth
/// degrade, which would be split-brain (contrast `prime`'s founding-miss
/// fallback, where nothing existed to land on).
///
/// **An op does not publish an anvil an enclosing op holds open (bl-1266,
/// store-scoped since bl-aac7).** A plugin that shells `bl` on THIS store
/// (bl-chore's `claim.post` mint was the shipped case) inserts a whole op —
/// seal AND push — into the middle of its parent's post phase, so without this
/// the nested push publishes the PARENT's not-yet-final commit; a later
/// `claim.post` failure then un-seals only the LOCAL store (`git reset --hard`),
/// and the next `bl sync` fast-forwards the repudiated op straight back. Nothing
/// is lost by waiting: a push publishes a branch TIP, so the nested seal rides
/// the parent's own trailing push (the tracker sorts LAST, §14) — one push per op
/// TREE, still last, and §14's *"core never pushes, so there is nothing remote to
/// chase"* becomes a theorem instead of an accident of hook order. The predicate
/// is the held-store chain, not depth ([`Env::nested`]): a nested `bl -C` on a
/// store no enclosing op holds has no parent push to ride, so it publishes.
pub fn push(b: &Binding, env: &Env) -> io::Result<()> {
    if env.nested(&b.store) {
        return Ok(()); // the enclosing op holds this anvil open — it publishes
    }
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    reject_option_like(remote)?;
    reject_option_like(&b.tasks_branch)?;
    let store = Path::new(&b.store);
    match git(store, &["push", remote, &b.tasks_branch]) {
        Ok(_) => {
            published(store);
            Ok(())
        }
        Err(_) => reconcile(b, env, remote),
    }
}

/// THE reconcile (bl-3616 §3 / bl-21ab): fetch the remote tip, `git rebase` the
/// store's unpublished seals onto it IN the checkout, push. Shared by the
/// per-op reject path ([`push`] — one seal) and [`sync`] (every seal). Four
/// outcomes, each stated once:
/// - the remote is UNREACHABLE — fail OPEN ([`fail_open`]): the store stays
///   ahead, nothing is lost, the next reachable op or `sync` publishes;
/// - the tip is not a store — the §16 migration window ([`warn_legacy`]),
///   work stays local, the legacy ref is never rewritten;
/// - the rebase CONFLICTS — the same ball changed on both sides. Abort the
///   rebase (the local seals are exactly as they were), name the ball, and
///   refuse ([`conflict`]) — the one contention that is semantically meaningful
///   and must not be auto-resolved: two claims of one ball IS claim contention
///   (the later loses), a close against a remote update is a human's call. No
///   field-wise merge, no CRDT (§7);
/// - clean — publish, unless an enclosing op holds this store open
///   (`Env::nested`, bl-1266); a push still rejected after a clean rebase is not
///   contention (a re-race — re-run — or a denied push), and stays fail-closed.
///
/// An unpublished seal's sha is scratch across this (bl-3616 §6.1): content,
/// trailers, timestamps and order survive the rebase, commit ids do not, and
/// nothing on `main` pins one — the seen-token is a BLOB sha, the journal
/// walks by path. The tracker sorts LAST (§14), so no plugin sees the
/// pre-rebase `commit` after this runs.
fn reconcile(b: &Binding, env: &Env, remote: &str) -> io::Result<()> {
    let store = Path::new(&b.store);
    let branch = b.tasks_branch.as_str();
    match fetch_tip(store, remote, branch) {
        Err(e) => {
            fail_open(remote, &e);
            return Ok(());
        }
        Ok(false) => {
            warn_legacy(remote, branch);
            return Ok(());
        }
        Ok(true) => {}
    }
    if let Err(e) = git(store, &["rebase", "FETCH_HEAD"]) {
        let contended = unmerged_balls(store);
        let _ = git(store, &["rebase", "--abort"]);
        return Err(conflict(&b.store, remote, branch, &contended, &e));
    }
    if env.nested(&b.store) {
        return Ok(());
    }
    git(store, &["push", remote, branch]).map(|_| published(store)).map_err(|e| {
        io::Error::other(format!(
            "push rejected by `{remote}` even after reconciling this store onto its `{branch}` tip — not a \
             same-ball conflict (the rebase was clean): either the remote moved again mid-op (re-run \
             the command) or the push is denied (permissions, a server hook). The op aborts and \
             un-seals; nothing was published ({e})"
        ))
    })
}

/// A push just landed: the remote tip IS this head — set the publication mark
/// (bl-439d) so the drift render reads current.
fn published(store: &Path) {
    if let Ok(head) = git(store, &["rev-parse", "HEAD"]) {
        super::drift::mark(store, &head);
    }
}

/// Is `remote`'s `branch` tip NOT a store — no `tasks/` tree at its root? That
/// is the §16 migration window (bl-868d): a hub still carrying the
/// PRE-greenfield legacy JSON store on the (colliding, §16) store-branch name,
/// awaiting the runbook's one-time human cutover. Such a tip is no upstream at
/// all — every store TIP carries `tasks/` by construction (§2, the founding
/// `.gitkeep`; the §16 cutover join keeps the greenfield tree) — so a failed
/// integrate/publish against it is the window, not contention: warn (the §12
/// diagnostic-never-authority pattern) and report `true` so the caller skips,
/// keeping work local and the legacy ref intact (cutover is the runbook's
/// explicit history join + fast-forward push, never a rewrite).
/// Identification must be POSITIVE: the tip is re-fetched here (`FETCH_HEAD`),
/// and any failure to read it reports `false` — the caller's own error stands.
pub(super) fn not_yet_cut_over(repo: &Path, remote: &str, branch: &str) -> bool {
    if tip_is_store(repo, remote, branch) != Some(false) {
        return false;
    }
    warn_legacy(remote, branch);
    true
}

/// The §16 migration-window warning — one spelling, shared by every site that
/// positively identified a legacy (non-store) tip.
fn warn_legacy(remote: &str, branch: &str) {
    eprintln!("tracker: `{remote}`'s `{branch}` is not a greenfield store (its tip has no tasks/) — a legacy store awaiting cutover, left intact; this checkout's store stays local until the ref is cut over (docs/migration-runbook.md)");
}

/// Is `remote`'s `branch` tip a greenfield STORE (`tasks/` at its root, §2)?
/// `None` = the tip could not be read at all (unreachable remote, absent
/// branch) — the caller decides; `Some(false)` is the §16 legacy window
/// [`not_yet_cut_over`] warns about; `Some(true)` is an ESTABLISHED store.
fn tip_is_store(repo: &Path, remote: &str, branch: &str) -> Option<bool> {
    fetch_tip(repo, remote, branch).ok()
}

/// Fetch `remote`'s `branch` to `FETCH_HEAD` and read its shape: `Ok(true)` an
/// established store (`tasks/` at the tip), `Ok(false)` a legacy tip, `Err` the
/// fetch itself failed (unreachable remote, absent branch) — the transport
/// signal [`reconcile`] fails open on. Positive identification by re-fetch,
/// shared by every reject-interpretation site.
fn fetch_tip(repo: &Path, remote: &str, branch: &str) -> io::Result<bool> {
    git(repo, &["fetch", remote, branch])?;
    if let Ok(tip) = git(repo, &["rev-parse", "FETCH_HEAD"]) {
        super::drift::mark(repo, &tip); // the remote tip, positively known as of now (bl-439d)
    }
    Ok(git(repo, &["cat-file", "-e", "FETCH_HEAD:tasks"]).is_ok())
}

/// Does `remote` already carry `branch`? `git ls-remote --heads` is the one
/// round-trip that answers "an upstream, if any" — sync's no-op gate and
/// prime's adopt-vs-found / clone-vs-bootstrap signal (§12/§13).
pub(super) fn remote_has_branch(cwd: &Path, remote: &str, branch: &str) -> io::Result<bool> {
    Ok(!git(cwd, &["ls-remote", "--heads", remote, branch])?.is_empty())
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

#[cfg(test)]
#[path = "remote_ops_push_tests.rs"]
mod push_tests;
