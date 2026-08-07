//! §11 delivery / worktree plugin — the DIRECT (local-squash) variant.
//!
//! A SIBLING of the tracker, default-wired but separate, so worktrees-without-
//! remote ⊥ remote-without-worktrees. It owns the deliverable CODE worktree —
//! a `git worktree` of the PROJECT repo on `work/<id>` — end to end. Base balls
//! never opens the project repo; "nothing on main / nothing in the project
//! tree" is therefore structural.
//!
//! **Kind-blind & stateless across ops.** The plugin NEVER branches on task
//! kind. The worktree path and branch are pure functions of `(binding, id)`
//! ([`crate::delivery_path::worktree_path`] / `work/<id>`); `<id>` rides EVERY
//! wire — `command.id` on a pre/post/rollback payload, the immutable `bl-id`
//! trailer once the op sealed ([`resolve_id`]) — so the plugin never reads
//! identity back out of the change worktree (§0 obligation 4; bl-a5f3). Every
//! hook recomputes its resource and checks the filesystem, so every hook is
//! idempotent by construction.
//!
//! **Worktrees materialize at CLAIM only (bl-c2bf).** A `work/<id>` worktree is
//! a durable filesystem entity, so `prime` re-creates nothing — re-priming a
//! lost worktree is `unclaim` + `claim`. `prime.post` is a diffless
//! checkout-lifecycle op (§13) that derives no `<id>`; the binary's prime path
//! only prunes settled `work/<id>` branches, outside this dispatch matrix.
//!
//! This module is the policy: [`dispatch`] maps `(op, phase, rolling_back)` to
//! the [`Repo`] act it performs (§11 hooks + §14 rollback). The git itself is
//! the [`Repo`] seam — [`crate::delivery_repo::Project`] is the real impl;
//! `dispatch` is unit-tested against a fake, so the branch matrix is covered
//! without a temp repo per case.

use std::io;
use std::path::Path;

use crate::message::Metadata;

/// The protocol self-description (`<bin> protocol`, §6): this plugin speaks
/// protocol 1 and handles the ops whose hooks it wires into — the four per-ball
/// lifecycle ops, `prime` for settled-branch pruning, and the `show` read-op (§6
/// read dispatch). balls reads it at install time, validates the wiring against
/// it, and never persists it.
pub const PROTOCOL_JSON: &str = r#"{"protocol":[1],"ops":["claim","unclaim","close","prime","show"]}"#;

/// The identities ONE delivery acted on (§11.1, bl-4eac) — everything a caller
/// needs to reconstruct provenance, and nothing balls stores to produce it.
/// Every field is a value [`Repo::deliver`] already computed for its own use.
///
/// The two `Option`s mean one thing between them: *the target already contained
/// everything the source had*. `source: None` is a source ref that was never
/// made (a claimed non-deliverable); `commit: None` is an empty deliverable or a
/// fully-merged source. A CONVERGED retry returns the STANDING delivery commit —
/// the one an earlier aborted close already landed — because provenance wants
/// the commit that exists, not the fact that this call did not mint it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivered {
    /// The target ref this delivery advanced (a branch name).
    pub target: String,
    /// The PINNED target tip (bl-8b89): the ancestry precondition's comparison
    /// point, the squash parent, and the CAS old-value — one read, four uses.
    pub base: String,
    /// The source tip at delivery (after any capture commit). `None` when the
    /// source ref does not exist.
    pub source: Option<String>,
    /// The delivery commit. `None` when nothing landed.
    pub commit: Option<String>,
}

/// The project-repo git acts the delivery hooks need, behind a seam so
/// [`dispatch`] is testable without a real repo. Each is idempotent — it
/// recomputes from `(path, branch)` and checks the filesystem (§11).
pub trait Repo {
    /// `claim.post`: create the code worktree at `path` on `branch`
    /// (create-if-absent). A non-deliverable that was claimed gets a harmless
    /// empty worktree.
    fn materialize(&self, path: &Path, branch: &str) -> io::Result<()>;
    /// `unclaim.post`: remove the worktree DIRECTORY if present; KEEP `branch`
    /// — unclaim is a HANDOFF, and committed work on the branch is what the
    /// next claimant re-materializes onto and a later close still delivers
    /// (the bl-65e0 contract). Discarding it is the holder's explicit
    /// `git branch -D`.
    fn release(&self, path: &Path) -> io::Result<()>;
    /// `close.post` + `rollback claim.post` (§14): remove the worktree AND
    /// delete `branch`. On a claim rollback it is the transactional undo of a
    /// just-made claim; on `close.post` it is the deletion the op can prove is
    /// lossless, because the squash and the seal have both already landed
    /// (bl-ce3b — see [`dispatch`]).
    fn discard(&self, path: &Path, branch: &str) -> io::Result<()>;
    /// The integration branch a delivery squashes onto — the DEFAULT target
    /// (the project repo's own HEAD branch, §11), used by every ball that does
    /// not nest ([`target_branch`]).
    fn integration(&self) -> io::Result<String>;
    /// Create `branch` at `base` if it does not exist yet; a no-op when it does.
    /// The LAZY MINT of a target ref (bl-7b71): the first child to claim into an
    /// epic brings `work/<epic>` into being at the integration head — a bare ref,
    /// nothing to orphan, no worktree. Minting there and forking it is
    /// bit-identical to forking the integration branch directly, so this is a
    /// naming, not a new code path.
    fn mint(&self, branch: &str, base: &str) -> io::Result<()>;
    /// `close.pre` deliver (direct): capture any pending worktree work onto
    /// `branch`, REQUIRE the pinned `integration` tip to be an ancestor of it
    /// already (bl-a1a4 — reconciliation is the source owner's job; a stale
    /// source refuses before anything merges, gates, squashes or moves),
    /// run the project repo's own
    /// pre-commit gate on that exact tree (bl-ee85 — the squash is plumbing, so
    /// without this the close would bypass the hook every porcelain commit
    /// runs; a failure aborts the close before the seal), then squash `branch`
    /// → `integration` as ONE commit carrying `message` — whose SUBJECT LINE is
    /// the tagged ball title and whose body is the author's work context
    /// ([`crate::delivery_message::compose`]), so it is unbounded text and
    /// never rides argv (bl-a500). A no-op when the worktree/branch is absent or
    /// carries no changes (the empty deliverable, §11) — and CONVERGENT ON
    /// RETRY (§14): when a `marker` commit already sits on `integration` since
    /// `branch` forked, this incarnation's delivery landed (an earlier aborted
    /// close, bl-430e, or a forge squash-merge) and deliver SKIPS the squash —
    /// IFF the delivery commit CONTAINS the branch's content; a branch carrying
    /// content beyond it (the bl-65e0 handoff) ABORTS loudly instead of
    /// stranding the work (bl-c231). Returns the [`Delivered`] identities in
    /// EVERY arm — the no-ops included, which is what makes an empty or
    /// converged delivery as provenance-legible as a freshly landed one
    /// (bl-4eac).
    fn deliver(&self, path: &Path, branch: &str, integration: &str, message: &str, marker: &str) -> io::Result<Delivered>;
    /// The author's substantive `work/<id>` commit messages for the delivery
    /// message (bl-b9a6): every NON-MERGE commit on `branch` since it forked
    /// from `integration`, oldest-first. Empty when the branch is absent (never
    /// worked) or carries only merge folds. Read by [`crate::delivery_message`]
    /// BEFORE `deliver` runs, so it sees only the author's own commits.
    fn work_messages(&self, branch: &str, integration: &str) -> io::Result<Vec<String>>;
    /// Is the invocation path (`root`) a git repository at all — BARE (the
    /// common balls deployment) or with a work tree? The delivery PRECONDITION
    /// (bl-4a88): every other act shells out to git against `root`, so a `root`
    /// that is not a git repo makes the whole `work/<id>` lifecycle unusable.
    /// Surfaced explicitly and early — a clean abort on claim.post / close.pre
    /// ([`crate::delivery_precondition::require_repo`]), a warning on prime.post — instead of git's raw
    /// `fatal: not a git repository` from the first worktree call.
    fn is_git_repo(&self) -> io::Result<bool>;
}

/// The resolved facts one hook acts on — the derived worktree, its branch, and
/// the delivery commit's `subject` / `marker`. Assembled by the binary edge
/// from the §7 wire + env.
pub struct Spec<'a> {
    pub worktree: &'a Path,
    pub branch: &'a str,
    pub subject: &'a str,
    /// The close's `-m` note, when given — free BODY narration under the tagged
    /// `subject`, never a subject override (§5; bl-9961). `None` on every op but
    /// a close that carried `-m`.
    pub override_msg: Option<&'a str>,
    pub marker: &'a str,
    /// The §7 `command.target` (bl-7b71): the id of the ball whose `work/<id>`
    /// ref this op delivers into. `None` — the flat case — is the integration
    /// branch. Core derives it from the graph; the plugin only turns it into a
    /// ref ([`target_branch`]).
    pub target: Option<&'a str>,
}

/// The ref this op forks from and folds back into (bl-7b71): the target's
/// `work/<id>` when the ball nests, else [`Repo::integration`] — which survives
/// as the DEFAULT, not a rival (it is not, and never was, hardcoded to `main`).
/// A nested target is minted at the integration head if it does not exist yet,
/// so the first child into an epic needs no prior epic claim.
pub fn target_branch(repo: &dyn Repo, target: Option<&str>) -> io::Result<String> {
    let Some(id) = target else { return repo.integration() };
    let branch = crate::delivery_path::work_branch(id);
    repo.mint(&branch, &repo.integration()?)?;
    Ok(branch)
}

/// Run the hook `(op, phase)` — or its rollback when `rolling_back` is `Some`
/// (§14) — against `repo`. Unknown hooks no-op (the plugin acts only where it
/// is wired).
pub fn dispatch(op: &str, phase: &str, rolling_back: bool, repo: &dyn Repo, spec: &Spec) -> io::Result<()> {
    match (op, phase, rolling_back) {
        ("claim", "post", false) => {
            fork(repo, spec)?;
            repo.materialize(spec.worktree, spec.branch)
        }
        // The hook wire has no return channel (§6), so the close.pre delivery's
        // identities are dropped here — a LINKING caller that wants them reaches
        // the same delivery through [`crate::attempt`] (bl-4eac).
        ("close", "pre", false) => crate::delivery_message::deliver_close(repo, spec).map(|_| ()),
        // UNCLAIM releases the worktree and KEEPS the branch: the ball goes back
        // on the board, nothing was delivered, and committed work on the branch
        // is what the next claim re-materializes onto (bl-65e0).
        ("unclaim", "post", false) => repo.release(spec.worktree),
        // DISCARD — worktree AND branch — at the two moments the branch is
        // provably dead. `rollback claim.post` undoes a claim that never
        // happened. `close.post` (bl-ce3b) deletes what its own `close.pre`
        // already squashed onto the target: the op that delivered KNOWS it
        // delivered. Deferring that delete to `prime` made prime reconstruct
        // the fact from a `[bl-<id>]` marker on the INTEGRATION branch, which a
        // NESTED ball (delivering into `work/<parent>`) structurally never puts
        // there — so its branch leaked forever, one per closed child. Acting on
        // the fact where it is known dissolves the nested case instead of
        // teaching the archaeology to see it.
        //
        // Ordering is the whole safety argument for the close arm. The blanket
        // deferral bought insurance against deleting the ONLY copy of an
        // undelivered diff — real, and EXPIRED by the time this hook runs:
        // close.pre squashed and the seal landed before any `post` fires, and
        // an abort BEFORE the squash never reaches here. `prime`'s prune stays
        // as the backstop for a crash between the seal and this line.
        ("close", "post", false) | ("claim", "post", true) => repo.discard(spec.worktree, spec.branch),
        // close.pre rollback DECLINES (§14): the squash is the delivery's
        // BINDING commit point — a standing squash without a sealed close is
        // the bl-430e state and the retried close converges onto it, while the
        // old un-squash reset raced concurrent integration movement (bl-c231).
        // unclaim's release is re-creatable from the branch, so its rollback is
        // a no-op too — and so is close.post's discard, for the same reason the
        // discard is safe in the first place: what it deleted was already
        // squashed onto the target, so nothing a rollback could restore is
        // gone. The retried close meets an ABSENT branch and converges, since
        // `deliver` no-ops on it and reads the standing delivery instead (§14;
        // pinned end-to-end by `tests/half_close.rs`). Any unwired hook too.
        _ => Ok(()),
    }
}

/// A NESTED claim forks its work branch off the TARGET's ref rather than the
/// integration head (bl-7b71): mint `work/<id>` at the target branch before the
/// worktree materializes on it, so the child starts from the work it gates and
/// its close folds back into the same ref. A flat claim (no target) declines —
/// `worktree add -b` forks the repo's HEAD, exactly as it always did.
fn fork(repo: &dyn Repo, spec: &Spec) -> io::Result<()> {
    let Some(target) = spec.target else { return Ok(()) };
    let base = target_branch(repo, Some(target))?;
    repo.mint(spec.branch, &base)
}

/// The §11 path surfacing — the stdout line a hook prints, if any (the §6
/// product channel; balls forwards it verbatim). The path is NEVER stored: it is
/// recomputed per surfacing (derive-don't-store, §11; bl-0af4 deleted the staged
/// `delivery-worktree` field). `claim.post` prints the BARE path — the verb's
/// one product, the way `create` prints the id (the only moment a worktree
/// materializes, bl-c2bf). The `show` read-op (§6 read dispatch) prints a human
/// field line instead, folded into `bl show`'s render — and only when the
/// worktree actually `exists`: a released or other-machine claim has no local
/// worktree, and the plugin asserts nothing git doesn't know.
#[must_use]
pub fn surfaced(op: &str, phase: &str, rolling_back: bool, worktree: &Path, exists: bool) -> Option<String> {
    match (op, phase, rolling_back) {
        ("claim", "post", false) => Some(worktree.display().to_string()),
        ("show", "read", false) if exists => Some(format!("  {:<9}{}", "worktree", worktree.display())),
        _ => None,
    }
}

/// Resolve the op's task id — always from the WIRE, never from the change
/// worktree (§0 obligation 4: identity is carried, not re-derived; bl-a5f3).
/// A sealed op carries it as the immutable `bl-id` trailer in `metadata` (the
/// only channel a §6 read-op has, since a read wire has no `command`); every
/// other payload — `pre`, `post`, and either phase's rollback — carries
/// `command.id`, the ball core named at op-start. Neither present is a protocol
/// error: the caller wired this plugin onto an op that names no ball.
pub fn resolve_id(metadata: Option<&Metadata>, command_id: Option<&str>) -> io::Result<String> {
    if let Some(id) = metadata.and_then(|m| m.get("bl-id")).and_then(|v| v.first()) {
        return Ok(id.clone());
    }
    command_id
        .map(str::to_string)
        .ok_or_else(|| io::Error::other("no ball on the wire: neither `command.id` nor a sealed `bl-id` trailer (§7)"))
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
