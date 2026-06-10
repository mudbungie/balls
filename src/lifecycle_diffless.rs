//! §13 diffless ops — `sync` and `prime`, which author no ball-file diff and so
//! "skip steps 1/3/5" of the §8 shape (no change worktree, no seal). An
//! `impl Engine` block split out of [`crate::lifecycle`]; as a child module it
//! reaches that module's private `run_phase`/`unwind`/[`SealRecord`] seams.
//!
//! - [`Engine::diffless`] runs `pre`→`post` once against ONE checkout (the store
//!   for `sync`), reading the anvil tip before/after `pre` and threading those §13
//!   facts to `post`. The op can still MOVE the tip — `sync/pre`'s ff advances
//!   `tasks_branch` — so the before/after pair is real.
//! - [`Engine::fixpoint`] is `prime`'s bounded loop (§12, bl-0a23): run `pre`
//!   against the LANDING, let core `materialize` the store, repeat until the
//!   configured `tasks_branch` stops moving (the `step` closure returns
//!   converged) or the [`FIXPOINT_CAP`] pass bound aborts the op (bl-33db),
//!   THEN run `post` against the store. Core owns the loop — the dial
//!   is the config branch core controls, so convergence needs no §7 return
//!   channel. `pre` runs against the landing because on a first prime the store is
//!   not materialized yet; `post` (the tracker's fetch-ff + push) runs against the
//!   store that `materialize` has by then laid down. No §5 message and no anvil
//!   facts cross to `post` here — the tracker's push reads neither.

use super::{run_phase, unwind, Engine, OpError, SealRecord};
use crate::log::Level;
use crate::op::Phase;
use crate::registry::PluginRef;
use crate::verb::Verb;
use std::io;
use std::path::Path;

/// The fixpoint pass cap (§8, bl-33db): a convergence loop still moving after
/// this many passes ABORTS the op — fail, not silent, mirroring the §6 depth
/// cap's disposition. The §6 cap cannot bound this loop (depth grows DOWN the
/// invocation tree; these passes iterate ACROSS it at one depth), so the loop
/// carries its own bound. Convergence normally takes 1–2 passes; only a `pre`
/// participant rewriting the dial on every pass ever nears the cap.
pub const FIXPOINT_CAP: u32 = 32;

impl Engine<'_> {
    /// Run a DIFFLESS op (§8 "skip steps 1/3/5"): pre/post against the `checkout`
    /// (the STORE for `sync`, §13), no worktree, no seal. Tier 1 is empty, so the
    /// unwind is reverse plugin rollback only (§13/§14). Unlike the seal path
    /// there is no commit to land, but the op can still MOVE the anvil tip — a
    /// `pre` participant (sync ff) advances `tasks_branch`. So balls reads the tip
    /// before `pre` and after it and threads those as the §13
    /// `previous_commit`/`commit` facts to `post` (and any `post`-phase rollback),
    /// metadata-less: a diffless op authors no §5 message, so `message` is `None`
    /// and the wire omits `metadata`. No shipped plugin reads them yet; a sync/post
    /// cache-rebuild participant will.
    pub fn diffless(&self, op: Verb, checkout: &Path, pre: &[PluginRef], post: &[PluginRef]) -> Result<(), OpError> {
        self.log.record(Level::Info, "core", None, "begin");
        let mut ran = Vec::new();
        let mut moved = None;
        let result = self.diffless_inner(op, checkout, pre, post, &mut ran, &mut moved);
        match &result {
            Ok(()) => self.log.record(Level::Info, "core", None, "done"),
            Err(e) => {
                unwind(self.plugins, op, checkout, &ran, moved.as_ref());
                self.log.record(Level::Error, "core", None, &format!("abort {e}"));
            }
        }
        result
    }

    /// The fallible pre→post body of a diffless op. Captures the anvil tip before
    /// `pre` and after it (§13), records those facts in `moved` so a post-abort
    /// unwind hands `post`-phase rollbacks the same shape (mirroring `Trace::seal`
    /// on the mutating path), and threads them — metadata-less — to `post`.
    fn diffless_inner(
        &self,
        op: Verb,
        checkout: &Path,
        pre: &[PluginRef],
        post: &[PluginRef],
        ran: &mut Vec<(PluginRef, Phase)>,
        moved: &mut Option<SealRecord>,
    ) -> Result<(), OpError> {
        let previous_commit = self.anvil.head().map_err(OpError::Anvil)?;
        run_phase(self.plugins, op, Phase::Pre, checkout, pre, None, ran)?;
        let commit = self.anvil.head().map_err(OpError::Anvil)?;
        let record = moved.insert(SealRecord { previous_commit, commit, message: None });
        run_phase(self.plugins, op, Phase::Post, checkout, post, Some(&record.facts()), ran)
    }

    /// Run `prime`'s bounded fixpoint (§12, bl-0a23). Each pass runs `pre` against
    /// `pre_dir` (the LANDING — the store may not be materialized yet) then calls
    /// `step`, core's between-phase work: `materialize` the store for the now-known
    /// `tasks_branch` and report the dial — `None` ⇒ it held (converged), `Some`
    /// carries the value that moved. The loop is core's, driven by that signal,
    /// never a plugin return value (§7), and bounded by [`FIXPOINT_CAP`] (bl-33db):
    /// a dial still moving at the cap ABORTS, the error naming the op and the
    /// oscillating value. When it settles, `post` runs against `post_dir` (the
    /// materialized store — the tracker's fetch-ff + push). `step`'s failure is an
    /// [`OpError::Substrate`] abort; any abort unwinds the run plugins in reverse
    /// (no seal, no §7 facts — `prime`'s push reads neither). The whole op logs one
    /// begin/done/abort.
    pub fn fixpoint(
        &self,
        op: Verb,
        pre_dir: &Path,
        post_dir: &Path,
        pre: &[PluginRef],
        post: &[PluginRef],
        step: &mut dyn FnMut() -> io::Result<Option<String>>,
    ) -> Result<(), OpError> {
        self.log.record(Level::Info, "core", None, "begin");
        let mut ran = Vec::new();
        // The fallible body: run `pre` then `step` until `step` reports converged,
        // then run `post`. Neither phase carries §7 facts (`None`) — prime's push
        // reads no seal. An immediate closure scopes the `&mut ran` borrow so the
        // unwind below can read it back.
        let result = (|| {
            let mut dial = String::new();
            for _ in 0..FIXPOINT_CAP {
                run_phase(self.plugins, op, Phase::Pre, pre_dir, pre, None, &mut ran)?;
                match step().map_err(OpError::Substrate)? {
                    None => return run_phase(self.plugins, op, Phase::Post, post_dir, post, None, &mut ran),
                    Some(moved) => dial = moved,
                }
            }
            Err(OpError::Substrate(io::Error::other(format!(
                "fixpoint pass cap ({FIXPOINT_CAP}) reached at {}.pre — the dial kept moving (last `{dial}`); aborting (§8)",
                op.token()
            ))))
        })();
        match &result {
            Ok(()) => self.log.record(Level::Info, "core", None, "done"),
            Err(e) => {
                unwind(self.plugins, op, pre_dir, &ran, None);
                self.log.record(Level::Error, "core", None, &format!("abort {e}"));
            }
        }
        result
    }
}
