//! Lifecycle-event dispatcher for legacy-plugin participants.
//!
//! One entry point per "kind" of event: `dispatch_push` for the
//! claim/review/close/update flows that today fire `Plugin::push`, and
//! `dispatch_sync` for the standalone `bl sync` invocation. Both walk
//! the active config, wrap each enabled plugin in a
//! `LegacyPluginParticipant`, and route through `participant::run`.
//!
//! There is no parallel dispatcher: the old `run_plugin_push` and
//! `run_plugin_sync` are gone. Anything that fires plugins comes
//! through here.

use super::participant::{LegacyOutcome, LegacyPluginParticipant};
use super::types::SyncReport;
use super::LegacyPushContribution;
use crate::error::Result;
use crate::negotiation::{Accepted, NegotiationResult};
use crate::participant::{self, Event, EventCtx, Participant};
use crate::store::Store;
use crate::task::Task;

/// Fire all subscribed legacy plugins for a push-shaped event
/// (claim/review/close/update). Collects each participant's outcome
/// plus the `CommitPolicy` it returned, then routes through the
/// apply-time planner so Required+Suppress is rejected, Batch
/// coalesces, and custom messages get the `plugin: <name>: ` prefix.
/// Today's legacy plugins all return the default policy, so the
/// observable behavior — one save plus one default commit per event
/// — is byte-identical to the pre-CommitPolicy dispatcher.
pub fn dispatch_push(
    store: &Store,
    task: &Task,
    event: Event,
    identity: &str,
) -> Result<()> {
    debug_assert!(matches!(
        event,
        Event::Claim | Event::Review | Event::Close | Event::Update
    ));
    let cfg = store.load_config()?;
    let mut contributions = Vec::new();
    for (name, entry) in cfg.plugins.iter().filter(|(_, e)| e.enabled) {
        let participant =
            LegacyPluginParticipant::from_entry(store, name.clone(), entry, None);
        if !participant.subscriptions().contains(&event) {
            continue;
        }
        let failure_policy = participant.failure_policy(event);
        let ctx = EventCtx {
            event,
            store,
            task_id: &task.id,
            identity,
        };
        if let NegotiationResult::Ok(Accepted {
            outcome: LegacyOutcome::Push(Some(r)),
            commit_policy,
        }) = participant::run(&participant, event, ctx)?
        {
            contributions.push(LegacyPushContribution {
                name: name.clone(),
                response: r,
                failure_policy,
                commit_policy,
            });
        }
    }
    super::apply_push_contributions(store, &task.id, &contributions)?;
    Ok(())
}

/// Fire all subscribed legacy plugins for the standalone sync event.
/// Returns the (plugin_name, SyncReport) pairs the caller applies via
/// `apply_sync_report`. Errs only on config-load failure (today's
/// `run_plugin_sync` semantics).
pub fn dispatch_sync(
    store: &Store,
    filter: Option<&str>,
    identity: &str,
) -> Result<Vec<(String, SyncReport)>> {
    let cfg = store.load_config()?;
    let mut reports = Vec::new();
    for (name, entry) in cfg.plugins.iter().filter(|(_, e)| e.enabled) {
        // Every legacy plugin subscribes to Sync (SPEC §11), so no
        // subscription gate here — the sync_on_change=false branch
        // still exposes the plugin to standalone `bl sync`.
        let participant = LegacyPluginParticipant::from_entry(
            store,
            name.clone(),
            entry,
            filter.map(str::to_string),
        );
        let ctx = EventCtx {
            event: Event::Sync,
            store,
            task_id: filter.unwrap_or(""),
            identity,
        };
        if let Ok(NegotiationResult::Ok(Accepted {
            outcome: LegacyOutcome::Sync(Some(r)),
            ..
        })) = participant::run(&participant, Event::Sync, ctx)
        {
            reports.push((name.clone(), r));
        }
    }
    Ok(reports)
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
