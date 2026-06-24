//! §8 lifecycle contract — the verb-agnostic vocabulary the [`super::Engine`]
//! runs against: the [`BaseChange`] diff seam (§9), the [`Plugins`] subprocess
//! chain seam (§6/§7), the [`Sealed`] post facts (§7), and the [`OpError`] abort
//! taxonomy (§14). Pure types, no engine internals — lifted here so the engine
//! file stays orchestration.

use std::io;
use std::path::Path;

use crate::op::Phase;
use crate::registry::PluginRef;
use crate::verb::Verb;

/// The op's base diff — the verb's contribution (§9). Staged into the change
/// worktree at Author (§8.1); the §5 commit message is built at Seal (§8.3) by
/// RE-READING the post-`pre` tree, so a `pre` plugin that reassigned the id is
/// reflected. bl-dfbd implements one per deliverable verb.
pub trait BaseChange {
    /// (§8.1) Stage the base diff into the change worktree `dir`.
    fn stage(&self, dir: &Path) -> io::Result<()>;
    /// (§8.3) Re-read `dir` after `pre` ran and render the §5 commit message
    /// (final id/state) the seal will commit.
    fn finalize(&self, dir: &Path) -> io::Result<String>;
    /// Does the change carry `-m` narration? The §5 free body lives ONLY in
    /// the sealed commit, so when this is true the engine refuses the no-op
    /// seal (converging on the existing tip would silently drop the note,
    /// bl-cf93). Default `false`: only `update` can stage a byte-identical
    /// tree (create mints a file, claim/unclaim flip `claimant`, close
    /// deletes), so it alone overrides.
    fn narrated(&self) -> bool {
        false
    }
}

/// What the op moved, threaded to every `post` reactor and any `post`-phase
/// rollback so the §7 post payload can carry it: the new commit, the tip it
/// landed on, and the §5 commit `message` (the plugin seam parses its trailers
/// into `metadata` — the engine stays §5-agnostic). `None` on `pre` (nothing is
/// sealed yet — the id is not assigned, §7).
///
/// On a DIFFLESS op (§13) there is no seal and no §5 message, so `message` is
/// `None`: the facts degrade to the checkout tip before/after the op
/// (`previous_commit`/`commit`), and `post` carries them metadata-less.
#[derive(Debug, Clone, Copy)]
pub struct Sealed<'a> {
    pub commit: &'a str,
    pub previous_commit: &'a str,
    pub message: Option<&'a str>,
}

/// The plugin chain (§6/§7) as a seam: run ONE plugin in a phase, or roll one
/// back. The lifecycle owns ORDER (the resolved set) and the reverse-order
/// unwind (§14); this seam owns the subprocess + wire (bl-5d56). `sealed` is
/// `Some` on `post` — and on EVERY rollback once the op sealed (§14: the id
/// rides "post/rollback from the sealed §5 trailer"), carrying the §7 post facts.
/// `rollback` returns nothing — best-effort, exit IGNORED (§14), so it can never
/// abort the unwind.
pub trait Plugins {
    /// Run `plugin` for `op`/`phase` against `dir`. `Err` aborts the op.
    fn run(
        &self,
        plugin: &PluginRef,
        op: Verb,
        phase: Phase,
        dir: &Path,
        sealed: Option<&Sealed>,
    ) -> io::Result<()>;
    /// Best-effort undo of `plugin`'s `phase` contribution (§14 `rolling_back`).
    fn rollback(&self, plugin: &PluginRef, op: Verb, phase: Phase, dir: &Path, sealed: Option<&Sealed>);
}

/// Why an op aborted. The engine maps each failing step here, then unwinds.
#[derive(Debug)]
pub enum OpError {
    /// A [`BaseChange`] stage/finalize failed (before the seal).
    Author(io::Error),
    /// An [`crate::git::Anvil`] git act (open/seal/head) failed.
    Anvil(io::Error),
    /// A core substrate step the engine drives between phases failed — the
    /// `materialize` `prime` runs between its `pre` and `post` (§12, bl-0a23),
    /// or `prime/pre` moved `tasks_branch` (the consent violation, bl-698d).
    Substrate(io::Error),
    /// A [`Plugins::run`] returned non-zero — the named plugin aborted the op.
    Plugin { name: String, source: io::Error },
    /// The §8.3 seal validation refused: a CHANGED `tasks/*.md` no longer
    /// parses (bl-528c). Carries the rendered refusal — file, last pre plugin,
    /// parse error — built where the facts live ([`super::validate`]).
    Invalid(String),
    /// The op carried `-m` narration but the seal converged on the existing
    /// tip (nothing changed — the no-op seal, §13): a note's only home is a
    /// commit, so converging would silently drop it (bl-cf93).
    Narration,
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpError::Author(e) => write!(f, "authoring the base change failed: {e}"),
            OpError::Anvil(e) => write!(f, "sealing onto the anvil failed: {e}"),
            OpError::Substrate(e) => write!(f, "materializing the store failed: {e}"),
            // The source already names the locus ("plugin X aborted the op…",
            // [`crate::plugin`]) — re-prefixing it here stuttered (bl-3ddb).
            OpError::Plugin { source, .. } => write!(f, "{source}"),
            OpError::Invalid(msg) => f.write_str(msg),
            OpError::Narration => write!(
                f,
                "nothing changed, so nothing sealed — the -m note rides only a commit and would be lost; retry in a second or drop -m"
            ),
        }
    }
}

impl std::error::Error for OpError {}
