//! §8 op lifecycle + §14 rollback — the verb-agnostic engine.
//!
//! balls authors a base change, an ordered plugin chain acts on it, balls
//! SEALS it (commit + integrate, atomically — [`crate::git`]), and plugins
//! react; any abort unwinds the whole op in reverse (§14). Two collaborators
//! are seams this engine owns the contract for: [`BaseChange`] (the verb's diff
//! authoring — §9, bl-dfbd) and [`Plugins`] (the subprocess plugin chain —
//! §6/§7, bl-5d56). The skeleton ships the orchestration and the real git seal;
//! the plugin chain has no production impl yet ("no real plugins yet").
//!
//! Mutating ops run the full Author → Pre → Seal → Post → Teardown shape;
//! diffless ops (reads, sync/prime) "skip steps 1/3/5" — pre/post run
//! against the store/landing checkout, no worktree and no seal (§8). One rule governs the
//! unwind: every plugin that ran a phase for THIS op rolls back in reverse,
//! THEN core un-seals its own tier-1 change — discard the worktree on a
//! pre-abort, `git reset` the anvil on a post-abort (§14). The committed
//! `[hooks]` plugin set (§6 — `config/plugins.toml`, bl-8540) is resolved once at
//! op-start (the §6 snapshot) and passed in, so the engine never reads config itself.

use std::io;
use std::path::Path;

use crate::git::Anvil;
use crate::log::{Level, Log};
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
    /// An [`Anvil`] git act (open/seal/head) failed.
    Anvil(io::Error),
    /// A core substrate step the engine drives between phases failed — the
    /// `materialize` the `prime` fixpoint runs between `pre` passes (§12, bl-0a23).
    Substrate(io::Error),
    /// A [`Plugins::run`] returned non-zero — the named plugin aborted the op.
    Plugin { name: String, source: io::Error },
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpError::Author(e) => write!(f, "authoring the base change failed: {e}"),
            OpError::Anvil(e) => write!(f, "sealing onto the anvil failed: {e}"),
            OpError::Substrate(e) => write!(f, "materializing the store failed: {e}"),
            OpError::Plugin { name, source } => write!(f, "plugin {name} aborted the op: {source}"),
        }
    }
}

impl std::error::Error for OpError {}

/// What the op moved, owned for the §14 unwind: the prior tip to reset back to,
/// the new commit, and the §5 message — enough to hand a `post`-phase rollback
/// the same [`Sealed`] facts its forward run saw (§7). `message` is `None` on a
/// diffless op (§13): no seal ran, so there is no §5 message to parse, and the
/// facts the record renders are the checkout tip before/after the op.
struct SealRecord {
    previous_commit: String,
    commit: String,
    message: Option<String>,
}

impl SealRecord {
    /// Borrow this record as the [`Sealed`] facts passed across the seam.
    fn facts(&self) -> Sealed<'_> {
        Sealed {
            commit: &self.commit,
            previous_commit: &self.previous_commit,
            message: self.message.as_deref(),
        }
    }
}

/// What core did this op, for the §14 unwind: the plugins that ran (in order,
/// each tagged with its phase), whether the change worktree was opened, and the
/// seal record once the seal landed (`Some` ⇒ a post-abort un-seals to its tip).
#[derive(Default)]
struct Trace {
    ran: Vec<(PluginRef, Phase)>,
    opened: bool,
    seal: Option<SealRecord>,
}

/// The §8 engine: an anvil, a plugin chain, and the op's [`Log`] sink. It
/// emits the op-level lifecycle records (begin/seal/abort, §6), interleaved with
/// the per-plugin `invoke`/envelope records the [`Plugins`] seam writes there.
pub struct Engine<'a> {
    anvil: &'a dyn Anvil,
    plugins: &'a dyn Plugins,
    log: &'a Log,
}

impl<'a> Engine<'a> {
    /// Build an engine over an anvil, a plugin chain, and the op's log sink.
    pub fn new(anvil: &'a dyn Anvil, plugins: &'a dyn Plugins, log: &'a Log) -> Self {
        Self { anvil, plugins, log }
    }

    /// Run a MUTATING op (§8 steps 1–6): Author → Pre → Seal → Post → Teardown,
    /// with §14 rollback on any abort. `pre`/`post` are the committed `[hooks]`
    /// plugin sets resolved at op-start (§6). Logs `begin`/`seal`/`abort` (§6); returns the sha.
    pub fn seal(
        &self,
        base: &dyn BaseChange,
        op: Verb,
        change_dir: &Path,
        pre: &[PluginRef],
        post: &[PluginRef],
    ) -> Result<String, OpError> {
        self.log.record(Level::Info, "core", None, "begin");
        let mut trace = Trace::default();
        match self.run_inner(base, op, change_dir, pre, post, &mut trace) {
            Ok(sha) => {
                let _ = self.anvil.close(change_dir); // (5) teardown, best-effort
                self.log.record(Level::Info, "core", None, &format!("seal {sha}"));
                Ok(sha)
            }
            Err(e) => {
                self.rollback(op, change_dir, &trace);
                self.log.record(Level::Error, "core", None, &format!("abort {e}"));
                Err(e)
            }
        }
    }

    /// The fallible Author → Pre → Seal → Post body; `trace` records what ran so
    /// [`Engine::rollback`] can unwind it.
    fn run_inner(
        &self,
        base: &dyn BaseChange,
        op: Verb,
        change_dir: &Path,
        pre: &[PluginRef],
        post: &[PluginRef],
        trace: &mut Trace,
    ) -> Result<String, OpError> {
        self.anvil.open(change_dir).map_err(OpError::Anvil)?; // (1) make the place
        trace.opened = true;
        base.stage(change_dir).map_err(OpError::Author)?; // (1) stage the base
        run_phase(self.plugins, op, Phase::Pre, change_dir, pre, None, &mut trace.ran)?; // (2)
        let prev = self.anvil.head().map_err(OpError::Anvil)?;
        let message = base.finalize(change_dir).map_err(OpError::Author)?;
        let sha = self.anvil.seal(change_dir, &message).map_err(OpError::Anvil)?; // (3) SEAL
        // boundary crossed: record the seal so post (and any post-abort) gets §7 facts.
        let sealed = trace
            .seal
            .insert(SealRecord { previous_commit: prev, commit: sha.clone(), message: Some(message) })
            .facts();
        run_phase(self.plugins, op, Phase::Post, change_dir, post, Some(&sealed), &mut trace.ran)?; // (4)
        Ok(sha)
    }

    /// §14 unwind: roll every run plugin back in reverse, THEN core un-seals its
    /// tier-1 change — `git reset` the anvil on a post-abort, discard the
    /// change worktree always. Core's un-seal is local, so its errors are
    /// swallowed (best-effort; the op already failed).
    fn rollback(&self, op: Verb, change_dir: &Path, trace: &Trace) {
        unwind(self.plugins, op, change_dir, &trace.ran, trace.seal.as_ref());
        if let Some(record) = &trace.seal {
            let _ = self.anvil.unseal(&record.previous_commit); // post-abort: reset the anvil
        }
        if trace.opened {
            let _ = self.anvil.close(change_dir); // discard the change worktree
        }
    }
}

/// Run one phase's plugins in resolved hook-list order, recording each success on
/// `ran` (a failing plugin cleaned up inline, so it is NOT recorded — §14).
fn run_phase(
    plugins: &dyn Plugins,
    op: Verb,
    phase: Phase,
    dir: &Path,
    list: &[PluginRef],
    sealed: Option<&Sealed>,
    ran: &mut Vec<(PluginRef, Phase)>,
) -> Result<(), OpError> {
    for plugin in list {
        plugins
            .run(plugin, op, phase, dir, sealed)
            .map_err(|source| OpError::Plugin { name: plugin.name.clone(), source })?;
        ran.push((plugin.clone(), phase));
    }
    Ok(())
}

/// Roll back every recorded plugin run in strict reverse execution order,
/// regardless of which phase it ran — the op is the unit of atomicity (§14).
/// EVERY rollback gets the [`Sealed`] facts once the seal landed — §14's id
/// rule ("post/rollback from the sealed §5 trailer") makes no phase split, and
/// a post-abort leaves the change worktree CLEAN, so a pre-phase rollback
/// (the delivery un-squash) starved of the trailer cannot re-derive its id
/// from changed task files and silently no-ops (bl-430e). On a pre-abort there
/// is no seal: `sealed` is `None` and the worktree is still dirty to read.
fn unwind(plugins: &dyn Plugins, op: Verb, dir: &Path, ran: &[(PluginRef, Phase)], seal: Option<&SealRecord>) {
    let sealed = seal.map(SealRecord::facts);
    for (plugin, phase) in ran.iter().rev() {
        plugins.rollback(plugin, op, *phase, dir, sealed.as_ref());
    }
}

/// §13 diffless ops (`sync`/`prime`) — pre/post against a checkout, no seal —
/// live in a sibling, an `impl Engine` block reaching this module's private
/// `run_phase`/`unwind`/`SealRecord` seams.
#[path = "lifecycle_diffless.rs"]
mod diffless;
pub use diffless::FIXPOINT_CAP;

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
