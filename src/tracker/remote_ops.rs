//! §12/§13 remote ops: `sync` (import) and `push` (publish). `sync` imports on
//! the explicit `bl sync` (and inside prime); `push` publishes after every
//! mutating op. Currency is OPTIMISTIC (mutate → push, bl-336a): there is no
//! pre-pull — a stale store surfaces atomically as the push's non-ff reject
//! (E5), and recovery is `bl sync` + retry — a forward to sync's verdict, not a
//! promise: the store may hold commits the remote never took, and then the
//! import refusal owns the exit ([`import_refused`], bl-4945). Both are no-ops in a stealth
//! (no-remote) repo: with no remote there is nothing to talk to, which is the
//! structural opt-out (§12).

use super::git::git;
use super::payload::Binding;
use super::Env;
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
/// non-ff IS the contention signal, so there is no separate contention probe;
/// on the checked-out branch it speaks in balls' voice ([`import_refused`],
/// bl-3129) rather than git's. Nothing is pushed, so a partial sync leaves the
/// branch at the old or the new tip, never wedged (§13 rollback).
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
        if git(store, &["merge", "--ff-only", "FETCH_HEAD"]).is_err() {
            let refused = || import_refused(&b.store, remote, branch);
            return not_yet_cut_over(store, remote, branch).then_some(()).ok_or_else(refused);
        }
    } else {
        // The refspec form's own non-ff is git's to spell: that one command is
        // fetch AND ref move, so its failure is ambiguous (unreachable remote,
        // absent ref, non-ff) and only the merge above is a positive loss.
        git(store, &["fetch", remote, &format!("{branch}:{branch}")])?;
    }
    Ok(())
}

/// A REFUSED STORE IMPORT, in balls' voice (bl-3129) — [`crate::git`]'s seal
/// rejection (bl-fa89) and `delivery_repo::acts::commit_swap`'s (bl-a3bb),
/// one layer out at the remote.
///
/// The fetched tip failed to fast-forward, which is §13 working: the import is
/// ff-only by contract (no union, no merge, no force), so the refusal means the
/// remote's history and this store's are no longer one line — and NOTHING moved,
/// neither imported nor overwritten. Raw git ("fatal: Not possible to
/// fast-forward, aborting", "Your local changes … would be overwritten") reads
/// as damage rather than as the two facts it is.
///
/// Unlike the seal's, this refusal is not always transient, and the sentence says
/// so. The optimistic §12 cycle (mutate → push, and a rejected push UN-SEALS,
/// tests/claim_race.rs) leaves the store non-diverged, so the ordinary cause is a
/// concurrent local `bl` whose seal was in flight across this fetch — a re-run
/// converges once it settles. A store that really does hold an unpublished
/// commit (a crash between seal and push, a hand-edited checkout) keeps refusing,
/// and the operator must reconcile it; naming both is the difference between an
/// instruction and a loop. No retry in core — the retry is one command (§14).
///
/// This is the ONE place that spells the EXIT (bl-4945), because it is the one
/// that detects the state: naming it without naming a way out still loops — push's
/// E5 sends the operator here, so its own sentence forwards to this verdict rather
/// than promising a convergence sync cannot always deliver. The exit is the
/// operator's, and it is stated as a choice, not a fix balls will apply: republish
/// the unpublished commits or discard them. balls never merges the two histories —
/// the ff-only contract IS the store's one-line-of-history invariant (§13), so an
/// automatic merge/rebase here would be core deciding an outcome only the operator
/// can weigh (whose ops those commits are, whether they still apply). The handles
/// are the ones the failed fetch just left in the store: `FETCH_HEAD` is the moved
/// remote tip, so `FETCH_HEAD..<branch>` is exactly the unpublished set.
fn import_refused(store: &str, remote: &str, branch: &str) -> io::Error {
    io::Error::other(format!(
        "`{remote}`'s `{branch}` moved and this store could not take the fast-forward — nothing was \
         imported and nothing local was changed. Re-run `bl sync`: it converges once a concurrent \
         `bl` settles its in-flight seal, and keeps refusing while this store carries commits the \
         remote never took. Reconciling those is yours — balls never merges the two histories: \
         `git -C {store} log FETCH_HEAD..{branch}` lists them (FETCH_HEAD is the moved remote tip), \
         then either rebase them onto FETCH_HEAD and push, or `git -C {store} reset --hard \
         FETCH_HEAD` to discard them"
    ))
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
/// branch) — the caller's own error stands; `Some(false)` is the §16 legacy
/// window [`not_yet_cut_over`] warns about; `Some(true)` is an ESTABLISHED
/// store, the E5 precondition. Positive identification by re-fetch
/// (`FETCH_HEAD`), shared by both reject-interpretation sites.
fn tip_is_store(repo: &Path, remote: &str, branch: &str) -> Option<bool> {
    git(repo, &["fetch", remote, branch]).ok()?;
    Some(git(repo, &["cat-file", "-e", "FETCH_HEAD:tasks"]).is_ok())
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
/// never rewritten (cutover is the runbook's explicit history join, published
/// as an ordinary fast-forward).
///
/// **A NESTED op does not publish (bl-1266).** An op publishes only if it is the
/// OUTERMOST `bl` in its invocation tree ([`Env::nested`]). A plugin that shells
/// `bl` (the shipped case: bl-chore's `claim.post` mint) inserts a whole op —
/// seal AND push — into the middle of its parent's post phase, so without this
/// the nested push publishes the PARENT's not-yet-final commit; a later
/// `claim.post` failure then un-seals only the LOCAL store (`git reset --hard`),
/// and the next `bl sync` fast-forwards the repudiated op straight back. Nothing
/// is lost by waiting: a push publishes a branch TIP, so the nested seal rides
/// the parent's own trailing push (the tracker sorts LAST, §14) — one push per op
/// TREE, still last, and §14's *"core never pushes, so there is nothing remote to
/// chase"* becomes a theorem instead of an accident of hook order.
pub fn push(b: &Binding, env: &Env) -> io::Result<()> {
    if env.nested() {
        return Ok(()); // the enclosing op holds this anvil open — it publishes
    }
    let Some(remote) = b.remote.as_deref() else {
        return Ok(());
    };
    reject_option_like(remote)?;
    reject_option_like(&b.tasks_branch)?;
    let store = Path::new(&b.store);
    if let Err(e) = git(store, &["push", remote, &b.tasks_branch]) {
        return match tip_is_store(store, remote, &b.tasks_branch) {
            // The §16 migration window — warn + keep the work local.
            Some(false) => {
                warn_legacy(remote, &b.tasks_branch);
                Ok(())
            }
            // E5 proper: the store is ESTABLISHED, so the reject is contention
            // (or revoked perms). Lead with the recovery — `bl sync` then
            // re-run the command — so the worn half-close path (bl-547f) reads
            // as a recoverable convergence, not a raw non-ff dump the user
            // mistakes for a broken close. But that two-step is not a PROMISE
            // (bl-4945): it converges because a rejected push un-seals, leaving
            // the store behind the remote and sync's ff-only free to run — and
            // a store that ALREADY carried an unpublished commit (a crash
            // between seal and push, the bl-547f shape) stays diverged past the
            // un-seal, so sync refuses and the promised loop never exits. So
            // the sentence forwards to sync's VERDICT instead of predicting it;
            // [`import_refused`] owns the state and the way out, in one place.
            Some(true) => Err(io::Error::other(format!(
                "push rejected: the remote store moved ahead, so this change did not publish — run `bl sync` (it converges the contention, or refuses and names what this store holds that the remote never took), then re-run the command ({e})"
            ))),
            // Unreadable tip (unreachable remote, absent branch): the push's
            // own error stands — never a silent skip, never a misnamed E5.
            None => Err(e),
        };
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

#[cfg(test)]
#[path = "remote_ops_push_tests.rs"]
mod push_tests;
