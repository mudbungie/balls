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
//! ([`crate::delivery_path::worktree_path`] / `work/<id>`); `<id>` rides the post wire (the immutable
//! `bl-id` trailer) or — on a pre hook, where the id is not sealed yet (§7) —
//! is read back from the single changed `tasks/<id>.md` in the change worktree
//! ([`resolve_id`]). Every hook recomputes its resource and checks the
//! filesystem, so every hook is idempotent by construction.
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

/// The project-repo git acts the delivery hooks need, behind a seam so
/// [`dispatch`] is testable without a real repo. Each is idempotent — it
/// recomputes from `(path, branch)` and checks the filesystem (§11).
pub trait Repo {
    /// `claim.post`: create the code worktree at `path` on `branch`
    /// (create-if-absent). A non-deliverable that was claimed gets a harmless
    /// empty worktree.
    fn materialize(&self, path: &Path, branch: &str) -> io::Result<()>;
    /// `unclaim.post` + `close.post`: remove the worktree DIRECTORY if
    /// present; KEEP `branch` (re-creatable; deleting it is deferred to
    /// prime, §14).
    fn release(&self, path: &Path) -> io::Result<()>;
    /// `rollback claim.post` (§14): remove the worktree AND delete `branch` —
    /// the transactional undo of a just-made claim.
    fn discard(&self, path: &Path, branch: &str) -> io::Result<()>;
    /// The integration branch a delivery squashes onto (default the project
    /// repo's own HEAD branch, §11).
    fn integration(&self) -> io::Result<String>;
    /// `close.pre` deliver (direct): capture any pending worktree work onto
    /// `branch`, fold `integration` into it, run the project repo's own
    /// pre-commit gate on the result (bl-ee85 — the squash is plumbing, so
    /// without this the close would bypass the hook every porcelain commit
    /// runs; a failure aborts the close before the seal), then squash `branch`
    /// → `integration` as ONE commit whose subject is `subject` (carrying the
    /// `[bl-id]` delivery tag). A no-op when the worktree/branch is absent or
    /// carries no changes (the empty deliverable, §11) — and CONVERGENT ON
    /// RETRY (§14): when a `marker` commit already sits on `integration` since
    /// `branch` forked, this incarnation's delivery landed (an earlier aborted
    /// close, bl-430e, or a forge squash-merge) and deliver SKIPS the squash —
    /// IFF the delivery commit CONTAINS the branch's content; a branch carrying
    /// content beyond it (the bl-65e0 handoff) ABORTS loudly instead of
    /// stranding the work (bl-c231).
    fn deliver(&self, path: &Path, branch: &str, integration: &str, subject: &str, marker: &str) -> io::Result<()>;
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
    /// ([`require_repo`]), a warning on prime.post — instead of git's raw
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
    /// The close's `-m` note, when given — a FULL override of the delivery
    /// message (bl-b9a6). `None` on every op but a close that carried `-m`.
    pub override_msg: Option<&'a str>,
    pub marker: &'a str,
}

/// Run the hook `(op, phase)` — or its rollback when `rolling_back` is `Some`
/// (§14) — against `repo`. Unknown hooks no-op (the plugin acts only where it
/// is wired).
pub fn dispatch(op: &str, phase: &str, rolling_back: bool, repo: &dyn Repo, spec: &Spec) -> io::Result<()> {
    match (op, phase, rolling_back) {
        ("claim", "post", false) => repo.materialize(spec.worktree, spec.branch),
        ("close", "pre", false) => crate::delivery_message::deliver_close(repo, spec),
        // Every worktree-deleting teardown is the same act — release the
        // worktree directory — whichever deleting op (close.post, unclaim)
        // triggers it.
        ("close" | "unclaim", "post", false) => repo.release(spec.worktree),
        ("claim", "post", true) => repo.discard(spec.worktree, spec.branch),
        // close.pre rollback DECLINES (§14): the squash is the delivery's
        // BINDING commit point — a standing squash without a sealed close is
        // the bl-430e state and the retried close converges onto it, while the
        // old un-squash reset raced concurrent integration movement (bl-c231).
        // close.post teardown + unclaim release are re-creatable from the
        // branch, so their rollback is a no-op too (§14); any unwired hook too.
        _ => Ok(()),
    }
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

/// Resolve the op's task id. A post hook carries it as the sealed `bl-id`
/// trailer in `metadata`; a pre hook does not (the id is not on the pre wire,
/// §7), so it is read back from the single changed `tasks/<id>.md` the op
/// staged — `changed` lists those paths (lazily: git is only run on the pre
/// path). Zero or many changed task files is a protocol error.
pub fn resolve_id(
    metadata: Option<&Metadata>,
    changed: impl FnOnce() -> io::Result<Vec<String>>,
) -> io::Result<String> {
    if let Some(id) = metadata.and_then(|m| m.get("bl-id")).and_then(|v| v.first()) {
        return Ok(id.clone());
    }
    let ids: Vec<String> = changed()?
        .iter()
        .filter_map(|p| p.strip_prefix("tasks/").and_then(|s| s.strip_suffix(".md")))
        .map(str::to_string)
        .collect();
    match ids.as_slice() {
        [id] => Ok(id.clone()),
        other => Err(io::Error::other(format!("expected exactly one changed task file, found {}", other.len()))),
    }
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
