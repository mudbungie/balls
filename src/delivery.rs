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
//! `<id>`: instead it scans the terminus checkout for every task still claimed
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
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::encoding::percent_encode;
use crate::layout::Xdg;
use crate::message::Metadata;

/// The protocol self-description (`<bin> protocol`, §6): this plugin speaks
/// protocol 1 and handles the five ops whose hooks it wires into (the four
/// per-ball lifecycle ops plus `prime` for re-materialization). balls reads it
/// at install time and never persists it.
pub const PROTOCOL_JSON: &str = r#"{"protocol":[1],"ops":["claim","unclaim","drop","close","prime"]}"#;

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
    /// prime/doctor, §14).
    fn release(&self, path: &Path) -> io::Result<()>;
    /// `rollback claim.post` (§14): remove the worktree AND delete `branch` —
    /// the transactional undo of a just-made claim.
    fn discard(&self, path: &Path, branch: &str) -> io::Result<()>;
    /// The integration branch a delivery squashes onto (default the project
    /// repo's own HEAD branch, §11).
    fn integration(&self) -> io::Result<String>;
    /// `close.pre` deliver (direct): capture any pending worktree work onto
    /// `branch`, then squash `branch` → `integration` as ONE commit whose
    /// subject is `subject` (carrying the `[bl-id]` delivery tag). A no-op when
    /// the worktree/branch is absent or carries no changes (the empty
    /// deliverable, §11).
    fn deliver(&self, path: &Path, branch: &str, integration: &str, subject: &str) -> io::Result<()>;
    /// `rollback close.pre` (§14): un-squash — reset `integration` to its parent
    /// IFF its tip is the delivery commit (its subject contains `marker`).
    /// Stateless and idempotent: a no-op delivery leaves no marked tip to undo.
    fn unsquash(&self, integration: &str, marker: &str) -> io::Result<()>;
}

/// The resolved facts one hook acts on — the derived worktree, its branch, the
/// user's shell cwd (for the guard), and the delivery commit's `subject` /
/// `marker`. Assembled by the binary edge from the §7 wire + env.
pub struct Spec<'a> {
    pub worktree: &'a Path,
    pub branch: &'a str,
    /// The user's shell working directory (`$PWD`), if known. Distinct from the
    /// normalized `invocation_path` that DERIVES the worktree — this is where
    /// the human actually stands, the only signal for the cwd guard. `None`
    /// when `$PWD` is unset (the guard then waves through, best-effort).
    pub cwd: Option<&'a Path>,
    pub subject: &'a str,
    pub marker: &'a str,
}

/// Run the hook `(op, phase)` — or its rollback when `rolling_back` is `Some`
/// (§14) — against `repo`. Unknown hooks no-op (the plugin acts only where it
/// is wired). The cwd guard makes EVERY worktree-deleting hook
/// (`close.{pre,post}` deliver/teardown AND `unclaim`/`drop.post` release)
/// refuse when `bl` was invoked from INSIDE the worktree balls is about to
/// tear down — this is where the "never close from inside the worktree" rule
/// lives, as a plugin precondition. It protects the agent: deleting a process's
/// cwd strands it (getcwd fails, relative paths resolve nowhere), and `bl
/// claim` drops the agent INSIDE this worktree, so a `drop`/`unclaim` from
/// there is just as dangerous as a `close`.
pub fn dispatch(op: &str, phase: &str, rolling_back: bool, repo: &dyn Repo, spec: &Spec) -> io::Result<()> {
    match (op, phase, rolling_back) {
        // `prime.post` re-materializes per still-claimed ball (the binary loops
        // and calls one dispatch per id) — the same act as a fresh `claim.post`.
        ("claim" | "prime", "post", false) => repo.materialize(spec.worktree, spec.branch),
        ("close", "pre", false) => {
            guard_cwd(spec)?;
            repo.deliver(spec.worktree, spec.branch, &repo.integration()?, spec.subject)
        }
        // Every worktree-deleting teardown: guard the agent, then release. One
        // arm — the act is identical whichever deleting op (close.post,
        // unclaim, drop) triggers it.
        ("close" | "unclaim" | "drop", "post", false) => {
            guard_cwd(spec)?;
            repo.release(spec.worktree)
        }
        ("claim", "post", true) => repo.discard(spec.worktree, spec.branch),
        ("close", "pre", true) => repo.unsquash(&repo.integration()?, spec.marker),
        // close.post teardown + unclaim/drop release are re-creatable from the
        // branch, so their rollback is a no-op (§14); any unwired hook too.
        _ => Ok(()),
    }
}

/// Refuse when the user's shell cwd (`$PWD`) is the worktree or a descendant of
/// it — the "never tear down from inside the worktree" precondition (§11),
/// fired on every hook that deletes the worktree (close deliver/teardown,
/// unclaim/drop release). Best-effort: an unknown `$PWD` waves through. balls
/// spawns the plugin with
/// its process cwd at the change worktree but leaves `$PWD` inherited from the
/// shell, so `$PWD` is the human's real location even though `current_dir` is
/// not (the id-resolution read uses `current_dir`; only this guard uses `$PWD`).
fn guard_cwd(spec: &Spec) -> io::Result<()> {
    if let Some(cwd) = spec.cwd {
        if cwd.starts_with(spec.worktree) {
            return Err(io::Error::other(format!(
                "refusing to deliver/tear down {}: cwd is inside the worktree — cd out and retry",
                spec.worktree.display()
            )));
        }
    }
    Ok(())
}

/// The derived code-worktree path (§11):
/// `$XDG_STATE_HOME/balls/plugins/<name>/<pct-enc(invocation_path)>/<id>/`.
/// balls prints the same path from the same formula — no return channel.
#[must_use]
pub fn worktree_path(xdg: &Xdg, plugin: &str, invocation_path: &str, id: &str) -> PathBuf {
    xdg.plugin_territory(plugin).join(percent_encode(invocation_path)).join(id)
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

/// The binding fields the plugin needs: where `bl` was invoked (§7/§11), and —
/// for `prime` — the terminus checkout whose `tasks/` the re-materialization
/// scans. `operating` is absent on the minimal per-ball payloads (which never
/// scan), so it is optional.
#[derive(Debug, Deserialize)]
pub struct WireBinding {
    pub invocation_path: String,
    #[serde(default)]
    pub operating: Option<String>,
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
