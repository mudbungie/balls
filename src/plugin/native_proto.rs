//! Per-event `Protocol` for native plugins (SPEC §5/§8). Split out of
//! `native_participant` so the *attempt state machine* lives apart
//! from the *registered participant adapter*.
//!
//! `NativeProtocol` holds the live working `Task` so the loop can act
//! on a `ProposeConflict` between attempts without bouncing back
//! through the store. A `ProposeConflict` flips the `AttemptClass` to
//! `Conflict` and SPEC §7 bounded retry kicks in; `remote_view` is
//! informational only (see `fetch_remote_view`).

use super::native_types::{
    CommitPolicyWire, EventCtxWire, ProposeConflict, ProposeOk, ProposeResponse,
};
use super::runner::{event_subcommand_arg, Plugin};
use crate::error::Result;
use crate::negotiation::{AttemptClass, CommitPolicy, Protocol};
use crate::participant::Event;
use crate::task::Task;
use serde_json::Value;

/// What a successful native negotiation hands back. The dispatcher
/// applies `task_projection` to the working task at apply time,
/// restricted by the participant's `projection`. The `commit_policy`
/// rides through the SPEC §10 planner.
#[derive(Debug, Clone)]
pub struct NativeOutcome {
    pub task_projection: Value,
    pub commit_policy: CommitPolicy,
}

/// Per-event Protocol state. Holds the live working `Task` so the
/// loop can fold `remote_view` from a conflict into it before
/// retrying without bouncing back through the store.
pub struct NativeProtocol<'a> {
    pub(crate) plugin: &'a Plugin,
    pub(crate) name: &'a str,
    pub(crate) event: Event,
    pub(crate) task: Box<Task>,
    /// Plugin's most recent successful `ok.task` payload — what the
    /// dispatcher will fold into the working Task on accept.
    pub(crate) accepted: Option<ProposeOk>,
    /// Most recent conflict; `fetch_remote_view` consumes this to
    /// merge `remote_view` into `self.task` before the loop retries.
    pub(crate) pending_conflict: Option<ProposeConflict>,
    pub(crate) retry_budget: usize,
    /// SPEC §5.1 — deliver the EventCtx side channel; `identity` = actor.
    pub(crate) wants_context: bool,
    pub(crate) identity: String,
}

impl<'a> NativeProtocol<'a> {
    /// Build a fresh per-event protocol. The participant adapter owns
    /// the only prod call site (`Participant::protocol`); keeping the
    /// fields private behind this constructor is what lets the struct
    /// live in a module separate from that adapter.
    pub(crate) fn new(
        plugin: &'a Plugin,
        name: &'a str,
        event: Event,
        task: Task,
        retry_budget: usize,
        wants_context: bool,
        identity: String,
    ) -> Self {
        Self {
            plugin,
            name,
            event,
            task: Box::new(task),
            accepted: None,
            pending_conflict: None,
            retry_budget,
            wants_context,
            identity,
        }
    }
}

impl Protocol for NativeProtocol<'_> {
    type Outcome = NativeOutcome;

    fn propose(&mut self) -> Result<AttemptClass> {
        if !self.plugin.auth_check() {
            return Ok(AttemptClass::Other(format!(
                "plugin `{}` auth-check failed",
                self.name
            )));
        }
        // SPEC §5.1: describe-gated. Only a plugin that asked for it
        // gets the side channel; everyone else is byte-identical.
        let ctx_json = if self.wants_context {
            Some(EventCtxWire::for_event(
                event_subcommand_arg(self.event),
                &self.identity,
                self.task.repo.clone(),
            )?)
        } else {
            None
        };
        let Some(resp) =
            self.plugin
                .propose(self.event, &self.task, ctx_json.as_deref())?
        else {
            return Ok(AttemptClass::Other(format!(
                "plugin `{}` propose returned no usable response",
                self.name
            )));
        };
        Ok(classify(
            resp,
            &mut self.accepted,
            &mut self.pending_conflict,
            self.name,
        ))
    }

    fn fetch_remote_view(&mut self) -> Result<()> {
        // The conflict report's `remote_view` is informational only.
        // SPEC §8: native plugins are responsible for their own
        // remote-state memory (typically a file under their
        // `--auth-dir`). Folding `remote_view` into the in-process
        // working Task would silently overwrite fields outside the
        // plugin's projection, defeating the disjoint-projection
        // composability that bl-8b71's mixed-config story relies on.
        // We just clear the pending slot so the next conflict has a
        // clean place to land.
        self.pending_conflict = None;
        Ok(())
    }

    fn pushed(&mut self) -> Self::Outcome {
        let ok = self.accepted.take().unwrap_or(ProposeOk {
            task: Value::Null,
            commit_policy: None,
        });
        let commit_policy = ok
            .commit_policy
            .map_or_else(CommitPolicy::default, CommitPolicyWire::into_policy);
        NativeOutcome {
            task_projection: ok.task,
            commit_policy,
        }
    }

    fn retry_budget(&self) -> usize {
        self.retry_budget
    }
}

pub(crate) fn classify(
    resp: ProposeResponse,
    accepted: &mut Option<ProposeOk>,
    pending_conflict: &mut Option<ProposeConflict>,
    name: &str,
) -> AttemptClass {
    let ProposeResponse { ok, conflict, reject, extra } = resp;
    // SPEC §8.1 precedence: a state-bearing accept/conflict wins; then
    // an explicit veto; then an unknown variant; then nothing.
    if let Some(ok) = ok {
        *accepted = Some(ok);
        return AttemptClass::Ok;
    }
    if let Some(conflict) = conflict {
        *pending_conflict = Some(conflict);
        return AttemptClass::Conflict;
    }
    if let Some(reject) = reject {
        // Distinct from Other: a deliberate veto carrying a reason.
        return AttemptClass::Reject(format!(
            "plugin `{name}` rejected: {}",
            reject.reason
        ));
    }
    // SPEC §13 seam 2: an unknown variant was captured in `extra`
    // rather than dropped — degrade to `Other` and name it so the
    // failure is diagnosable, not a mute "nothing returned".
    if !extra.is_empty() {
        let v = extra.keys().cloned().collect::<Vec<_>>().join(", ");
        return AttemptClass::Other(format!(
            "plugin `{name}` propose returned unknown variant(s): {v}"
        ));
    }
    AttemptClass::Other(format!(
        "plugin `{name}` propose returned neither ok nor conflict"
    ))
}

#[cfg(test)]
#[path = "native_proto_test_helpers.rs"]
mod test_helpers;

#[cfg(test)]
#[path = "native_participant_proto_tests.rs"]
mod proto_tests;
