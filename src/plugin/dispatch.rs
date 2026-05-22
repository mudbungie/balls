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

/// Everything a lifecycle command hands the push dispatcher. The
/// `task_before`/`commit`/`override_tokens` triple feeds the SPEC §5.1
/// side channel and §11 audit; `overrides` (skip/required) drives
/// per-invocation subscription resolution.
pub struct DispatchInput<'a> {
    pub store: &'a Store,
    pub task_before: Option<&'a Task>,
    pub task: &'a Task,
    pub event: Event,
    pub identity: &'a str,
    pub commit: Option<&'a str>,
    pub overrides: &'a InvocationOverrides,
    pub override_tokens: &'a [String],
}

/// What the command consumes back. `skipped` is the native
/// best-effort participants whose negotiation didn't land —
/// `(name, verbatim reason)`, folded into `task.sync_status` by the
/// apply step. Legacy-shim skips are deliberately absent so SPEC §12
/// byte-identity holds for unmodified configs.
#[derive(Debug, Default)]
pub struct DispatchOutcome {
    pub skipped: Vec<(String, String)>,
}

/// One plugin's dispatch result before composition.
enum DispatchItem {
    Contribution(PushContribution),
    /// Native best-effort skip — recorded in `sync_status`.
    Skipped(String, String),
    /// Not subscribed, a legacy-shim skip, or gating-staged (bl-a46d):
    /// nothing to apply and nothing to record.
    Inert,
}

/// Fire all subscribed plugins for a push-shaped event
/// (create/claim/review/close/update). Each plugin runs once, native
/// (when `describe` succeeds) or legacy shim. Successful outcomes
/// funnel through the SPEC §10 planner; a required failure (incl. a
/// first-class `reject`, §8.1) propagates as `Err` for the command to
/// roll back; native best-effort skips are returned for `sync_status`.
pub fn dispatch_push(input: &DispatchInput) -> Result<DispatchOutcome> {
    let &DispatchInput {
        store,
        task_before,
        task,
        event,
        identity,
        commit,
        overrides,
        override_tokens,
    } = input;
    debug_assert!(matches!(
        event,
        Event::Create | Event::Claim | Event::Review | Event::Close | Event::Update
    ));
    let cfg = store.load_project_config()?;
    let mut contributions = Vec::new();
    let mut skipped = Vec::new();
    for (name, entry) in cfg.plugins.iter().filter(|(_, e)| e.enabled) {
        let ctx = EventCtx::new(event, store, &task.id, identity).with_context(
            Some(task),
            task_before,
            commit,
            override_tokens,
        );
        match dispatch_one_push(store, name, entry, event, ctx, overrides)? {
            DispatchItem::Contribution(c) => contributions.push(c),
            DispatchItem::Skipped(n, reason) => skipped.push((n, reason)),
            DispatchItem::Inert => {}
        }
    }
    super::apply_push_contributions(store, &task.id, &contributions, &skipped)?;
    Ok(DispatchOutcome { skipped })
}

/// Decide which protocol a plugin uses and run one negotiation. A
/// plugin that responds to `describe` is routed through the native
/// protocol; otherwise the legacy shim. A native plugin that doesn't
/// subscribe to this event is `Inert` so the dispatcher does not
/// double-fire it via the shim.
fn dispatch_one_push(
    store: &Store,
    name: &str,
    entry: &crate::config::PluginEntry,
    event: Event,
    ctx: EventCtx<'_>,
    overrides: &InvocationOverrides,
) -> Result<DispatchItem> {
    let plugin = super::Plugin::resolve(store, name, entry);
    if let Some(describe) = plugin.describe()? {
        return run_native(store, name, entry, event, ctx, overrides, describe);
    }
    run_legacy(store, name, entry, event, ctx, overrides)
}

fn run_native(
    store: &Store,
    name: &str,
    entry: &crate::config::PluginEntry,
    event: Event,
    ctx: EventCtx<'_>,
    overrides: &InvocationOverrides,
    describe: super::native_types::DescribeResponse,
) -> Result<DispatchItem> {
    let participant = NativePluginParticipant::from_describe(
        store,
        name.to_string(),
        entry,
        None,
        overrides,
        describe,
    )?;
    if !participant.subscriptions().contains(&event) {
        return Ok(DispatchItem::Inert);
    }
    let failure_policy = participant.failure_policy(event);
    let projection = participant.projection().clone();
    // A required failure surfaces as `Err` from `participant::run`
    // (the negotiation's `classify_failure`); `?` carries it to the
    // command, which rolls the state branch back (SPEC §9). A native
    // best-effort skip is captured for `sync_status` (§8.1/§9);
    // gating staging is bl-a46d, treated inert here.
    match participant::run(&participant, event, ctx)? {
        NegotiationResult::Ok(Accepted {
            outcome: NativeOutcome { task_projection, commit_policy },
            ..
        }) => Ok(DispatchItem::Contribution(PushContribution {
            name: name.to_string(),
            projection,
            payload: ContributionPayload::Native(task_projection),
            failure_policy,
            commit_policy,
        })),
        NegotiationResult::Skipped(reason) => {
            Ok(DispatchItem::Skipped(name.to_string(), reason))
        }
        NegotiationResult::Staged(_) => Ok(DispatchItem::Inert),
    }
}

fn run_legacy(
    store: &Store,
    name: &str,
    entry: &crate::config::PluginEntry,
    event: Event,
    ctx: EventCtx<'_>,
    overrides: &InvocationOverrides,
) -> Result<DispatchItem> {
    // Legacy plugins honor §11 `--skip`/`--required` too, but their
    // skips stay silent: recording `sync_status` for an unmodified
    // config would add a state-branch commit where today there is
    // none, breaking SPEC §12 / §17.1 byte-identity. So a non-Ok
    // legacy outcome is `Inert`, never `Skipped`.
    let participant = LegacyPluginParticipant::resolved(
        store,
        name.to_string(),
        entry,
        None,
        overrides,
        None,
    );
    if !participant.subscriptions().contains(&event) {
        return Ok(DispatchItem::Inert);
    }
    let failure_policy = participant.failure_policy(event);
    let projection = Projection::external_only(name);
    if let NegotiationResult::Ok(Accepted {
        outcome: LegacyOutcome::Push(Some(r)),
        commit_policy,
    }) = participant::run(&participant, event, ctx)?
    {
        return Ok(DispatchItem::Contribution(PushContribution {
            name: name.to_string(),
            projection,
            payload: ContributionPayload::Legacy(r),
            failure_policy,
            commit_policy,
        }));
    }
    Ok(DispatchItem::Inert)
}

/// Fire all subscribed plugins for the standalone sync event. Returns
/// the (plugin_name, SyncReport) pairs the caller applies via
/// `apply_sync_report`. Errs only on config-load failure.
pub fn dispatch_sync(
    store: &Store,
    filter: Option<&str>,
    identity: &str,
) -> Result<Vec<(String, SyncReport)>> {
    let cfg = store.load_project_config()?;
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
        let ctx = EventCtx::new(Event::Sync, store, filter.unwrap_or(""), identity);
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

/// Observe-only `Drop` notification (SPEC §6.2). Drop changes nothing
/// a participant can negotiate, so this routes through the one
/// dispatch primitive but **discards every outcome and never
/// propagates an error**: a downed or rejecting observer must not
/// fail a local claim release (§2 soft policy, hard primitive). It
/// applies no contributions — drop is a notification, not a state
/// change. Legacy plugins never declare `Drop` (the legacy mapping
/// omits it) and so are silently skipped, keeping `bl drop`
/// byte-identical to pre-bl-ec62 for them. `required`/`gating` on
/// `drop` cannot reach here: config validation rejects them.
pub fn dispatch_drop(store: &Store, task: &Task, identity: &str) -> Result<()> {
    let cfg = store.load_project_config()?;
    let overrides = InvocationOverrides::default();
    for (name, entry) in cfg.plugins.iter().filter(|(_, e)| e.enabled) {
        let ctx = EventCtx::new(Event::Drop, store, &task.id, identity);
        let _ = dispatch_one_push(store, name, entry, Event::Drop, ctx, &overrides);
    }
    Ok(())
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
