//! Native plugin participants — SPEC §5/§8 wire impl. A plugin opts
//! into native participation by shipping `describe` and `propose`
//! subcommands (bl-8b71). The describe response carries the
//! projection and event subscriptions; the per-event negotiation
//! `Protocol` lives in the sibling `native_proto` module.
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

use super::native_types::DescribeResponse;
use super::runner::Plugin;
use crate::config::PluginEntry;
use crate::error::Result;
use crate::negotiation::FailurePolicy;
use crate::participant::{Event, EventCtx, Participant, Projection};
use crate::participant_config::{
    effective_subscriptions, InvocationOverrides, LocalPluginEntry,
};
use crate::store::Store;
use std::collections::BTreeMap;

// The per-event Protocol + its outcome live in `native_proto`.
// `NativeOutcome` is re-exported so `native_participant::NativeOutcome`
// (used by `dispatch` and the `plugin` facade) is unchanged.
pub use crate::plugin::native_proto::NativeOutcome;
use crate::plugin::native_proto::NativeProtocol;

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
    /// SPEC §5.1 — the plugin asked for the EventCtx side channel in
    /// its describe response. Off ⇒ byte-identical input to today.
    wants_context: bool,
}

/// Default budget per SPEC §7 — same as the git-remote participant.
pub const DEFAULT_NATIVE_RETRY_BUDGET: usize = 5;

impl NativePluginParticipant {
    /// Construct from a successful `describe`. Declared events are
    /// intersected with `effective_subscriptions` so SPEC §11 config
    /// layering applies as for legacy plugins. `Err` if the declared
    /// projection names an unknown field.
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
            wants_context: describe.wants_context,
        })
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
        // Prefer the command's in-hand post-image: a `close` archived
        // the task file pre-dispatch, so re-loading would miss it and
        // the plugin could never veto (SPEC §9). Non-push callers fall
        // back to the store.
        let task = match ctx.post {
            Some(t) => t.clone(),
            None => match ctx.store.load_task(ctx.task_id) {
                Ok(t) => t,
                Err(_) => return None,
            },
        };
        let proto = NativeProtocol::new(
            &self.plugin,
            &self.name,
            event,
            task,
            self.retry_budget,
            self.wants_context,
            ctx.identity.to_string(),
            ctx.task_before.map(|t| Box::new(t.clone())),
            ctx.commit.map(str::to_string),
            ctx.overrides.to_vec(),
        );
        Some(proto)
    }
}

#[cfg(test)]
#[path = "native_participant_describe_tests.rs"]
mod describe_tests;
