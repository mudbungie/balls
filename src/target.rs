//! The §11 DELIVERY TARGET derivation (bl-7b71) — which ref a ball's work forks
//! from and folds back into.
//!
//! > If the ball close-gates its LIVE parent — it has `parent = X` AND `X`
//! > carries the blocker `{this, on: close}` — the target is `X`. Otherwise the
//! > target is absent, meaning the integration branch.
//!
//! Flat delivery is the degenerate case: main is what a parentless ball targets,
//! and depth recurses for free. The target is derived at op time and NEVER
//! stored (§14 derive-don't-store) — it is pure graph arithmetic over the ball
//! and its parent, so it changes the moment the edge does.
//!
//! It lives in CORE, not in the delivery plugin, for one structural reason: the
//! gating edge sits on the PARENT's task file, and the plugin only ever sees the
//! ball riding the §7 wire (`current_state`). A plugin re-deriving nesting would
//! have to open the store and fork core's graph semantics into a second home. So
//! core derives and puts the id on the wire ([`crate::wire::Command::target`]);
//! the plugin turns it into `work/<id>`.
//!
//! Nesting needs BOTH coordinates. A bare `--parent` is containment only and
//! stays flat-to-main; an explicit `--blocks ID:close` on a NON-parent is pure
//! enforcement and never redirects delivery (two sibling features ordered by a
//! close-gate must keep delivering independently). Containment is what licenses
//! the redirection.
//!
//! `--subtask-of E` is both coordinates in one word (bl-e844) — the everyday way
//! a nested ball is filed.

use std::path::Path;

use crate::task::Task;
use crate::taskfile::read_task;
use crate::verb::Verb;

/// The op's delivery target id, read against the `store` checkout: `before` is
/// the ball at op-start (`None` on `create` — no ball, no target) and `id` is the
/// op's positional. `None` ⇒ the integration branch.
///
/// A parent whose file is gone is a CLOSED (or never-was) parent — a dangling,
/// display-only pointer (§10 absence-is-the-record), so it gates nothing and the
/// ball delivers flat. Reading it is the one IO here; the decision itself is
/// [`close_gated`].
pub(crate) fn derive(store: &Path, id: Option<&String>, before: Option<&Task>) -> Option<String> {
    let parent = before?.parent.as_deref()?;
    let live = read_task(store, parent).ok()?;
    close_gated(id?, &live).then(|| parent.to_string())
}

/// Does `id` close-gate `parent`? The nesting declaration: a `{id, on: close}`
/// edge on the parent — the parent cannot close until this ball does, so this
/// ball's work belongs on the parent's branch.
pub(crate) fn close_gated(id: &str, parent: &Task) -> bool {
    parent.blockers.iter().any(|b| b.id == id && b.on == Verb::Close)
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
