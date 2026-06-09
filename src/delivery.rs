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
//! ([`worktree_path`] / `work/<id>`); `<id>` rides the post wire (the immutable
//! `bl-id` trailer) or — on a pre hook, where the id is not sealed yet (§7) —
//! is read back from the single changed `tasks/<id>.md` in the change worktree
//! ([`resolve_id`]). Every hook recomputes its resource and checks the
//! filesystem, so every hook is idempotent by construction.
//!
//! **Per-session re-materialization (§11/§12).** `prime.post` carries no single
//! ball (it is a diffless checkout-lifecycle op, §13), so it does not derive one
//! `<id>`: instead it scans the anvil checkout for every task still claimed
//! by the actor ([`crate::delivery_repo::claimed_ids`]) and re-materializes each
//! one's worktree — the SAME `materialize` act `claim.post` performs, just
//! driven per-claimed-task. Create-if-absent makes it idempotent, so a prime on
//! a session whose worktrees already exist converges to a no-op.
//!
//! This module is the policy: [`dispatch`] maps `(op, phase, rolling_back)` to
//! the [`Repo`] act it performs (§11 hooks + §14 rollback). The git itself is
//! the [`Repo`] seam — [`crate::delivery_repo::Project`] is the real impl;
//! `dispatch` is unit-tested against a fake, so the branch matrix is covered
//! without a temp repo per case.

use std::io;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::layout::Xdg;
use crate::message::Metadata;

/// The protocol self-description (`<bin> protocol`, §6): this plugin speaks
/// protocol 1 and handles the ops whose hooks it wires into — the four per-ball
/// lifecycle ops, `prime` for re-materialization, and the `show` read-op (§6
/// read dispatch). balls reads it at install time, validates the wiring against
/// it, and never persists it.
pub const PROTOCOL_JSON: &str = r#"{"protocol":[1],"ops":["claim","unclaim","drop","close","prime","show"]}"#;

/// The project-repo git acts the delivery hooks need, behind a seam so
/// [`dispatch`] is testable without a real repo. Each is idempotent — it
/// recomputes from `(path, branch)` and checks the filesystem (§11).
pub trait Repo {
    /// `claim.post`: create the code worktree at `path` on `branch`
    /// (create-if-absent). A non-deliverable that was claimed gets a harmless
    /// empty worktree.
    fn materialize(&self, path: &Path, branch: &str) -> io::Result<()>;
    /// `unclaim/drop.post` + `close.post`: remove the worktree DIRECTORY if
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
    /// carries no changes (the empty deliverable, §11) — and RETRY-IDEMPOTENT:
    /// when a `marker` commit already sits on `integration` since `branch`
    /// forked, this incarnation's delivery landed in an earlier aborted close,
    /// and re-squashing would mint an empty duplicate (bl-430e), so deliver
    /// skips.
    fn deliver(&self, path: &Path, branch: &str, integration: &str, subject: &str, marker: &str) -> io::Result<()>;
    /// `rollback close.pre` (§14): un-squash — reset `integration` to its parent
    /// IFF its tip is the delivery commit (its subject contains `marker`).
    /// Stateless and idempotent: a no-op delivery leaves no marked tip to undo.
    fn unsquash(&self, integration: &str, marker: &str) -> io::Result<()>;
}

/// The resolved facts one hook acts on — the derived worktree, its branch, and
/// the delivery commit's `subject` / `marker`. Assembled by the binary edge
/// from the §7 wire + env.
pub struct Spec<'a> {
    pub worktree: &'a Path,
    pub branch: &'a str,
    pub subject: &'a str,
    pub marker: &'a str,
}

/// Run the hook `(op, phase)` — or its rollback when `rolling_back` is `Some`
/// (§14) — against `repo`. Unknown hooks no-op (the plugin acts only where it
/// is wired).
pub fn dispatch(op: &str, phase: &str, rolling_back: bool, repo: &dyn Repo, spec: &Spec) -> io::Result<()> {
    match (op, phase, rolling_back) {
        // `prime.post` re-materializes per still-claimed ball (the binary loops
        // and calls one dispatch per id) — the same act as a fresh `claim.post`.
        ("claim" | "prime", "post", false) => repo.materialize(spec.worktree, spec.branch),
        ("close", "pre", false) => {
            repo.deliver(spec.worktree, spec.branch, &repo.integration()?, spec.subject, spec.marker)
        }
        // Every worktree-deleting teardown is the same act — release the
        // worktree directory — whichever deleting op (close.post, unclaim,
        // drop) triggers it.
        ("close" | "unclaim" | "drop", "post", false) => repo.release(spec.worktree),
        ("claim", "post", true) => repo.discard(spec.worktree, spec.branch),
        ("close", "pre", true) => repo.unsquash(&repo.integration()?, spec.marker),
        // close.post teardown + unclaim/drop release are re-creatable from the
        // branch, so their rollback is a no-op (§14); any unwired hook too.
        _ => Ok(()),
    }
}

/// The §11 path surfacing — the stdout line a hook prints, if any (the §6
/// product channel; balls forwards it verbatim). The path is NEVER stored: it is
/// recomputed per surfacing (derive-don't-store, §11; bl-0af4 deleted the staged
/// `delivery-worktree` field). `claim.post` and each `prime.post`
/// re-materialization print the BARE path — the verb's one product, the way
/// `create` prints the id. The `show` read-op (§6 read dispatch) prints a human
/// field line instead, folded into `bl show`'s render — and only when the
/// worktree actually `exists`: a released or other-machine claim has no local
/// worktree, and the plugin asserts nothing git doesn't know.
#[must_use]
pub fn surfaced(op: &str, phase: &str, rolling_back: bool, worktree: &Path, exists: bool) -> Option<String> {
    match (op, phase, rolling_back) {
        ("claim" | "prime", "post", false) => Some(worktree.display().to_string()),
        ("show", "read", false) if exists => Some(format!("  {:<9}{}", "worktree", worktree.display())),
        _ => None,
    }
}

/// This binding's worktree territory (§11):
/// `$XDG_STATE_HOME/balls/plugins/<name>/<invocation_path>/`. Every `work/<id>`
/// worktree is an `<id>/` child; [`worktree_path`] joins one id onto it.
///
/// Unlike every other layout name (which percent-encodes its key into one
/// inspectable component, §1), this one MIRRORS the invocation path verbatim —
/// the leading `/` stripped so it nests rather than re-roots. The reason is
/// concrete: this subtree is the project's *code* worktree, where `cargo`/`rustc`
/// build, and `rust-lld` cannot open an output file whose path contains a `%`
/// (bl-f3e4). A percent-encoded ancestor would poison every link. Mirroring the
/// real path is at least as inspectable as encoding it (§1's actual goal — names
/// you can read, never a hash) and is always a valid filesystem path, since the
/// invocation path already is one. The git-data layouts (clones, tracker) keep
/// percent-encoding: nothing compiles there, so `%` is harmless.
#[must_use]
pub fn binding_territory(xdg: &Xdg, plugin: &str, invocation_path: &str) -> PathBuf {
    xdg.plugin_territory(plugin).join(invocation_path.trim_start_matches('/'))
}

/// Reject an `invocation_path` that is not a clean absolute path, BEFORE it is
/// mirrored by [`binding_territory`] (bl-2d6d). The mirror joins the path
/// verbatim — it gives up the `..`-neutralization percent-encoding gives the
/// clone layout — so a relative path or a `..` component would let the worktree
/// escape plugin territory. The delivery edge calls this once, at wire ingress,
/// before any worktree path is derived.
pub fn ensure_safe_invocation_path(p: &str) -> io::Result<()> {
    let path = Path::new(p);
    if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
        return Err(io::Error::other(format!(
            "unsafe invocation path (must be absolute, no '..'): {p:?}"
        )));
    }
    Ok(())
}

/// The derived code-worktree path (§11): the `<id>/` child of this binding's
/// [`binding_territory`]. balls prints the same path from the same formula — no
/// return channel. Pairs with [`work_branch`] — both derive from the same `<id>`
/// key, so §11 claimant-keying (`<key> = <id>` or `<id>-<claimant>`) is a single
/// edit across the pair, not a hunt for every `work/<id>` literal.
#[must_use]
pub fn worktree_path(xdg: &Xdg, plugin: &str, invocation_path: &str, id: &str) -> PathBuf {
    binding_territory(xdg, plugin, invocation_path).join(id)
}

/// The `work/<id>` branch this binding's worktree sits on (§11) — the BRANCH
/// half of the `(worktree_path, branch)` pair. Every site that derives one must
/// derive the other through these two helpers so they cannot drift; see
/// [`worktree_path`].
#[must_use]
pub fn work_branch(id: &str) -> String {
    format!("work/{id}")
}

/// The delivery commit subject: `<title> [<id>]`. The `[<id>]` tag is delivery
/// ground truth — the `delivered_in` query (§11) tag-scans the integration
/// branch for it, so it is also the [`Repo::unsquash`] marker.
#[must_use]
pub fn subject(title: &str, id: &str) -> String {
    format!("{title} [{id}]")
}

/// `[<id>]` — the delivery tag the squash subject carries and `unsquash` looks
/// for at the integration tip.
#[must_use]
pub fn marker(id: &str) -> String {
    format!("[{id}]")
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

/// The §7 fields the delivery plugin reads off stdin. balls only ever
/// serializes the wire ([`crate::wire`]); the plugin owns the matching
/// deserialize for the slice it needs — `invocation_path` (the project root),
/// the `bl-id` metadata, the ball `title` for the squash subject, and the
/// `rolling_back` tag.
#[derive(Debug, Deserialize)]
pub struct Wire {
    /// The invoking identity (`--as`). Only `prime` reads it (to pick out the
    /// actor's still-claimed balls); the per-ball ops act on a single derived
    /// id and ignore it, so it defaults empty when a payload omits it.
    #[serde(default)]
    pub actor: String,
    pub binding: WireBinding,
    #[serde(default)]
    pub metadata: Option<Metadata>,
    #[serde(default)]
    pub current_state: Option<WireState>,
    #[serde(default)]
    pub rolling_back: Option<String>,
}

/// The one binding field the plugin needs: where `bl` was invoked (§7/§11) —
/// the project-repo root the derived worktree paths hang off. The store
/// checkout `prime` scans is the diffless cwd balls invokes us in (§13), not a
/// wire field, so it is not carried here.
#[derive(Debug, Deserialize)]
pub struct WireBinding {
    pub invocation_path: String,
}

/// The one ball field the plugin needs: the title, for the squash subject.
#[derive(Debug, Default, Deserialize)]
pub struct WireState {
    #[serde(default)]
    pub title: String,
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
