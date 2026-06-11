//! §13 diffless ops — `sync` and `prime`, which author no ball-file diff and so
//! "skip steps 1/3/5" of the §8 shape (no change worktree, no seal). An
//! `impl Engine` block split out of [`crate::lifecycle`]; as a child module it
//! reaches that module's private `run_phase`/`unwind`/[`SealRecord`] seams.
//!
//! - [`Engine::diffless`] runs `pre`→`post` once against ONE checkout (the store
//!   for `sync`), reading the anvil tip before/after `pre` and threading those §13
//!   facts to `post`. The op can still MOVE the tip — `sync/pre`'s ff advances
//!   `tasks_branch` — so the before/after pair is real.
//! - [`Engine::prime`] is `prime`'s split-checkout pass (§12, bl-698d): run
//!   `pre` ONCE against the LANDING, let core `materialize` the store, then run
//!   `post` against the store. The configured `tasks_branch` may not MOVE across
//!   `pre` — no conformant plugin rewrites it (config crosses only by `install`,
//!   §12), so a moved name is a consent violation and ABORTS the op, fail-not-
//!   silent. The check is core's — the dial is the config branch core controls,
//!   no §7 return channel. `pre` runs against the landing because on a first
//!   prime the store is not materialized yet; `post` (the tracker's fetch-ff +
//!   push) runs against the store that `materialize` has by then laid down. No
//!   §5 message and no anvil facts cross to `post` here — the tracker's push
//!   reads neither.

use super::{run_phase, unwind, Engine, OpError, SealRecord};
use crate::log::Level;
use crate::op::Phase;
use crate::registry::PluginRef;
use crate::verb::Verb;
use std::io;
use std::path::Path;

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
        self.log.record(Level::Debug, "core", None, "begin");
        let mut ran = Vec::new();
        let mut moved = None;
        let result = self.diffless_inner(op, checkout, pre, post, &mut ran, &mut moved);
        match &result {
            Ok(()) => self.log.record(Level::Debug, "core", None, "done"),
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

    /// Run `prime`'s split-checkout pass (§12, bl-698d): `pre` ONCE against
    /// `pre_dir` (the LANDING — the store may not be materialized yet), then
    /// `step`, core's between-phase work: `materialize` the store for the
    /// configured `tasks_branch` and report the dial — `None` ⇒ it held, `Some`
    /// carries the value that moved. A moved dial ABORTS: no conformant plugin
    /// rewrites `tasks_branch` in `prime/pre` (config crosses only by `install`,
    /// §12/§13), so the check ENFORCES the consent rule — fail, not silent — the
    /// error naming the rule and the moved value. When the dial held, `post` runs
    /// against `post_dir` (the materialized store — the tracker's fetch-ff +
    /// push). `step`'s failure is an [`OpError::Substrate`] abort; any abort
    /// unwinds the run plugins in reverse (no seal, no §7 facts — `prime`'s push
    /// reads neither). A PLUGIN abort is rendered as the §12 catalog's E7 —
    /// "plugin failed during prime, rolled back K prior" (K = the plugins that
    /// had run and were unwound) — not the generic abort (bl-3ddb). The whole op
    /// logs one begin/done/abort.
    pub fn prime(
        &self,
        pre_dir: &Path,
        post_dir: &Path,
        pre: &[PluginRef],
        post: &[PluginRef],
        step: &mut dyn FnMut() -> io::Result<Option<String>>,
    ) -> Result<(), OpError> {
        self.log.record(Level::Debug, "core", None, "begin");
        let mut ran = Vec::new();
        // The fallible body: `pre`, `step`, the moved-dial check, `post`. Neither
        // phase carries §7 facts (`None`) — prime's push reads no seal. An
        // immediate closure scopes the `&mut ran` borrow so the unwind below can
        // read it back.
        let result = (|| {
            run_phase(self.plugins, Verb::Prime, Phase::Pre, pre_dir, pre, None, &mut ran)?;
            if let Some(moved) = step().map_err(OpError::Substrate)? {
                return Err(OpError::Substrate(io::Error::other(format!(
                    "prime.pre may not move tasks_branch (config now names `{moved}`) — config crosses only by install (§13); aborting"
                ))));
            }
            run_phase(self.plugins, Verb::Prime, Phase::Post, post_dir, post, None, &mut ran)
        })();
        match result {
            Ok(()) => {
                self.log.record(Level::Debug, "core", None, "done");
                Ok(())
            }
            Err(e) => {
                unwind(self.plugins, Verb::Prime, pre_dir, &ran, None);
                let e = match e {
                    // E7: name the prime-specific shape — K prior plugins unwound.
                    OpError::Plugin { name, source } => OpError::Plugin {
                        name,
                        source: io::Error::other(format!(
                            "plugin failed during prime, rolled back {} prior: {source}",
                            ran.len()
                        )),
                    },
                    e => e,
                };
                self.log.record(Level::Error, "core", None, &format!("abort {e}"));
                Err(e)
            }
        }
    }
}
