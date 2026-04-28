//! Lifecycle-event dispatcher for plugin participants.
//!
//! One entry point per "kind" of event: `dispatch_push` for the
//! claim/review/close/update flows, and `dispatch_sync` for the
//! standalone `bl sync` invocation. Each plugin is dispatched through
//! exactly one protocol — native if `Plugin::describe()` returns
//! `Some`, otherwise the legacy push/sync shim. Both protocols feed
//! into a single `PushContribution` stream so SPEC §10's commit-policy
//! planner sequences mixed configs in stable subscription order.
//!
//! There is no parallel dispatcher: `run_plugin_push` /
//! `run_plugin_sync` were collapsed into this module by bl-b1dd. The
//! native protocol slots in the same way — one `participant::run` call
//! per plugin per event, no special-cased control flow.

use super::native_participant::{NativeOutcome, NativePluginParticipant};
use super::participant::{LegacyOutcome, LegacyPluginParticipant};
use super::types::SyncReport;
use super::{ContributionPayload, PushContribution};
use crate::error::Result;
use crate::negotiation::{Accepted, NegotiationResult};
use crate::participant::{self, Event, EventCtx, Participant, Projection};
use crate::participant_config::InvocationOverrides;
use crate::store::Store;
use crate::task::Task;

/// Fire all subscribed plugins for a push-shaped event
/// (claim/review/close/update). Each plugin is dispatched once, via
/// either the native protocol (when `describe` succeeds) or the
/// legacy shim. Outcomes funnel into a single `PushContribution`
/// vector and through the SPEC §10 commit-policy planner so a config
/// that mixes the two protocols still produces a deterministic
/// commit sequence.
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
        let ctx = EventCtx {
            event,
            store,
            task_id: &task.id,
            identity,
        };
        if let Some(c) = dispatch_one_push(store, name, entry, event, ctx)? {
            contributions.push(c);
        }
    }
    super::apply_push_contributions(store, &task.id, &contributions)?;
    Ok(())
}

/// Decide which protocol a plugin uses and run one negotiation. A
/// plugin that responds to `describe` is routed through the native
/// protocol; otherwise we fall through to the legacy shim. A native
/// plugin that doesn't subscribe to this event returns `None` so the
/// dispatcher does not double-fire it via the shim.
fn dispatch_one_push(
    store: &Store,
    name: &str,
    entry: &crate::config::PluginEntry,
    event: Event,
    ctx: EventCtx<'_>,
) -> Result<Option<PushContribution>> {
    let plugin = super::Plugin::resolve(store, name, entry);
    if let Some(describe) = plugin.describe()? {
        return run_native(store, name, entry, event, ctx, describe);
    }
    run_legacy(store, name, entry, event, ctx)
}

fn run_native(
    store: &Store,
    name: &str,
    entry: &crate::config::PluginEntry,
    event: Event,
    ctx: EventCtx<'_>,
    describe: super::native_types::DescribeResponse,
) -> Result<Option<PushContribution>> {
    let participant = NativePluginParticipant::from_describe(
        store,
        name.to_string(),
        entry,
        None,
        &InvocationOverrides::default(),
        describe,
    )?;
    if !participant.subscriptions().contains(&event) {
        return Ok(None);
    }
    let failure_policy = participant.failure_policy(event);
    let projection = participant.projection().clone();
    if let NegotiationResult::Ok(Accepted {
        outcome: NativeOutcome { task_projection, commit_policy },
        ..
    }) = participant::run(&participant, event, ctx)?
    {
        return Ok(Some(PushContribution {
            name: name.to_string(),
            projection,
            payload: ContributionPayload::Native(task_projection),
            failure_policy,
            commit_policy,
        }));
    }
    Ok(None)
}

fn run_legacy(
    store: &Store,
    name: &str,
    entry: &crate::config::PluginEntry,
    event: Event,
    ctx: EventCtx<'_>,
) -> Result<Option<PushContribution>> {
    let participant =
        LegacyPluginParticipant::from_entry(store, name.to_string(), entry, None);
    if !participant.subscriptions().contains(&event) {
        return Ok(None);
    }
    let failure_policy = participant.failure_policy(event);
    let projection = Projection::external_only(name);
    if let NegotiationResult::Ok(Accepted {
        outcome: LegacyOutcome::Push(Some(r)),
        commit_policy,
    }) = participant::run(&participant, event, ctx)?
    {
        return Ok(Some(PushContribution {
            name: name.to_string(),
            projection,
            payload: ContributionPayload::Legacy(r),
            failure_policy,
            commit_policy,
        }));
    }
    Ok(None)
}

/// Fire all subscribed plugins for the standalone sync event. Returns
/// the (plugin_name, SyncReport) pairs the caller applies via
/// `apply_sync_report`. Errs only on config-load failure.
pub fn dispatch_sync(
    store: &Store,
    filter: Option<&str>,
    identity: &str,
) -> Result<Vec<(String, SyncReport)>> {
    let cfg = store.load_config()?;
    let mut reports = Vec::new();
    for (name, entry) in cfg.plugins.iter().filter(|(_, e)| e.enabled) {
        // Native plugins do not currently emit the standalone sync
        // report shape; the dispatcher always uses the legacy sync
        // wire here. A native plugin that also wants to participate
        // in `bl sync` ships a legacy-shaped `sync` subcommand
        // alongside its describe/propose pair.
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
