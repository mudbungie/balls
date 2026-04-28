//! Native plugin participants — SPEC §5/§8 wire impl. A plugin opts
//! into native participation by shipping `describe` and `propose`
//! subcommands (bl-8b71). The describe response carries the
//! projection and event subscriptions; propose runs once per attempt
//! and may return a clean `ok` result, a structured `conflict` for
//! the negotiation primitive to retry, or no usable data (treated as
//! `Other`).
//!
//! `NativePluginParticipant` implements the SPEC §5 `Participant`
//! contract on top of `runner::Plugin`. It mirrors the shape of the
//! legacy shim (`super::participant`) but with two key differences:
//!
//! - **Projection comes from the plugin, not from the shim default.**
//!   A native plugin can declare ownership of canonical fields or
//!   multiple `external.*` slices, and the dispatcher will route only
//!   those into the working task.
//! - **Conflicts retry.** A `ProposeConflict` flips the
//!   `AttemptClass` to `Conflict`, the loop folds the `remote_view`
//!   into the working task before the next propose, and SPEC §7
//!   bounded retry kicks in.
//!
//! Coexistence with legacy plugins: each enabled plugin is dispatched
//! through exactly one of the two protocols (native iff `describe`
//! returns Some). `dispatch::dispatch_push` decides per plugin and
//! never double-dispatches.

use super::native_types::{DescribeResponse, ProposeConflict, ProposeOk, ProposeResponse};
use super::runner::Plugin;
use crate::config::PluginEntry;
use crate::error::Result;
use crate::negotiation::{AttemptClass, CommitPolicy, FailurePolicy, Protocol};
use crate::participant::{Event, EventCtx, Participant, Projection};
use crate::participant_config::{
    effective_subscriptions, InvocationOverrides, LocalPluginEntry,
};
use crate::store::Store;
use crate::task::Task;
use serde_json::Value;
use std::collections::BTreeMap;

/// A native plugin lifted into a `Participant`. Holds the resolved
/// `Plugin` (paths/exe), the projection it declared via `describe`,
/// and the per-event failure-policy map resolved through SPEC §11
/// layered config — same shape as the legacy shim, so the dispatcher
/// can treat both kinds uniformly.
pub struct NativePluginParticipant {
    name: String,
    plugin: Plugin,
    projection: Projection,
    subscriptions: Vec<Event>,
    failure_policies: BTreeMap<Event, FailurePolicy>,
    retry_budget: usize,
}

/// Default budget per SPEC §7 — same as the git-remote participant
/// and the inline `claim_sync` retry loop bl-2148 introduced.
pub const DEFAULT_NATIVE_RETRY_BUDGET: usize = 5;

impl NativePluginParticipant {
    /// Construct from a successful `describe` response. The describe
    /// response declares which events the plugin can negotiate; the
    /// effective failure policies still come from
    /// `effective_subscriptions` so per-event config layering applies
    /// the same way it does for legacy plugins. Returns `Err` if the
    /// describe payload's projection contains an unknown field name.
    pub fn from_describe(
        store: &Store,
        name: String,
        entry: &PluginEntry,
        local: Option<&LocalPluginEntry>,
        invocation: &InvocationOverrides,
        describe: DescribeResponse,
    ) -> Result<Self> {
        let plugin = Plugin::resolve(store, &name, entry);
        let resolved = effective_subscriptions(&name, entry, local, invocation);
        // Subscriptions are the intersection of "what the plugin
        // declared in describe" and "what config subscribed it to".
        // Either side independently can opt out of an event without
        // forcing the other to know.
        let declared: std::collections::BTreeSet<Event> =
            describe.subscriptions.iter().copied().collect();
        let failure_policies: BTreeMap<Event, FailurePolicy> = resolved
            .into_iter()
            .filter(|(ev, _)| declared.contains(ev))
            .collect();
        let subscriptions: Vec<Event> = failure_policies.keys().copied().collect();
        let projection = describe.projection.into_projection()?;
        let retry_budget = describe
            .retry_budget
            .unwrap_or(DEFAULT_NATIVE_RETRY_BUDGET);
        Ok(Self {
            name,
            plugin,
            projection,
            subscriptions,
            failure_policies,
            retry_budget,
        })
    }
}

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
    plugin: &'a Plugin,
    name: &'a str,
    event: Event,
    task: Box<Task>,
    /// Plugin's most recent successful `ok.task` payload — what the
    /// dispatcher will fold into the working Task on accept.
    accepted: Option<ProposeOk>,
    /// Most recent conflict; `fetch_remote_view` consumes this to
    /// merge `remote_view` into `self.task` before the loop retries.
    pending_conflict: Option<ProposeConflict>,
    retry_budget: usize,
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
        let Some(resp) = self.plugin.propose(self.event, &self.task)? else {
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
            .map_or_else(CommitPolicy::default, super::native_types::CommitPolicyWire::into_policy);
        NativeOutcome {
            task_projection: ok.task,
            commit_policy,
        }
    }

    fn retry_budget(&self) -> usize {
        self.retry_budget
    }
}

fn classify(
    resp: ProposeResponse,
    accepted: &mut Option<ProposeOk>,
    pending_conflict: &mut Option<ProposeConflict>,
    name: &str,
) -> AttemptClass {
    match (resp.ok, resp.conflict) {
        (Some(ok), _) => {
            *accepted = Some(ok);
            AttemptClass::Ok
        }
        (None, Some(conflict)) => {
            *pending_conflict = Some(conflict);
            AttemptClass::Conflict
        }
        (None, None) => AttemptClass::Other(format!(
            "plugin `{name}` propose returned neither ok nor conflict"
        )),
    }
}

impl Participant for NativePluginParticipant {
    type Outcome = NativeOutcome;
    type Protocol<'a>
        = NativeProtocol<'a>
    where
        Self: 'a;

    fn name(&self) -> &str {
        &self.name
    }
    fn subscriptions(&self) -> &[Event] {
        &self.subscriptions
    }
    fn projection(&self) -> &Projection {
        &self.projection
    }
    fn failure_policy(&self, event: Event) -> FailurePolicy {
        self.failure_policies
            .get(&event)
            .copied()
            .unwrap_or(FailurePolicy::BestEffort)
    }
    fn protocol<'a>(
        &'a self,
        event: Event,
        ctx: EventCtx<'a>,
    ) -> Option<Self::Protocol<'a>> {
        let task = ctx.store.load_task(ctx.task_id).ok()?;
        Some(NativeProtocol {
            plugin: &self.plugin,
            name: &self.name,
            event,
            task: Box::new(task),
            accepted: None,
            pending_conflict: None,
            retry_budget: self.retry_budget,
        })
    }
}

/// Test-only constructor exposing private state of `NativeProtocol`
/// so unit tests drive the in-process branches without spawning.
#[cfg(test)]
impl<'a> NativeProtocol<'a> {
    pub(crate) fn __test_new(
        plugin: &'a Plugin, name: &'a str, event: Event, task: Task, retry_budget: usize,
    ) -> Self {
        Self {
            plugin, name, event, task: Box::new(task),
            accepted: None, pending_conflict: None, retry_budget,
        }
    }
    pub(crate) fn __test_record_ok(&mut self, ok: ProposeOk) { self.accepted = Some(ok); }
    pub(crate) fn __test_record_conflict(&mut self, c: ProposeConflict) {
        self.pending_conflict = Some(c);
    }
    pub(crate) fn __test_task_title(&self) -> String { self.task.title.clone() }
    pub(crate) fn __test_has_pending_conflict(&self) -> bool {
        self.pending_conflict.is_some()
    }
}

#[cfg(test)]
pub(crate) fn __test_classify(
    resp: ProposeResponse, accepted: &mut Option<ProposeOk>,
    pending_conflict: &mut Option<ProposeConflict>, name: &str,
) -> AttemptClass {
    classify(resp, accepted, pending_conflict, name)
}

#[cfg(test)]
#[path = "native_participant_test_helpers.rs"]
mod test_helpers;

#[cfg(test)]
#[path = "native_participant_describe_tests.rs"]
mod describe_tests;

#[cfg(test)]
#[path = "native_participant_proto_tests.rs"]
mod proto_tests;
