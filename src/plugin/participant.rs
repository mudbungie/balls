//! Legacy `balls-plugin-{name}` push/sync subprocess plugins exposed
//! as SPEC §5 Participants. The shim keeps every observable behavior
//! identical to today's direct dispatch:
//!
//! - Subscriptions derived from `sync_on_change` per SPEC §11 mapping
//!   (true → claim/review/close/update/sync; false → sync only).
//! - Projection covers only `external.<name>.*`; canonical fields are
//!   read but never owned (SPEC §5).
//! - Failure policy is `BestEffort` for every event — matches today's
//!   swallowed-failure behavior. Subprocess errors and missing auth
//!   collapse to `AttemptClass::Other`, which BestEffort absorbs as
//!   `NegotiationResult::Skipped`.
//! - Retry budget is 1: legacy plugins have no way to express
//!   recoverable conflicts (SPEC §8), so retrying a failed subprocess
//!   would just re-fail.
//!
//! The dispatcher in `super::dispatch` walks the active config,
//! constructs one participant per enabled plugin, and routes the
//! lifecycle event through `participant::run`. The old direct loop in
//! `commands/lifecycle.rs` has no parallel path: this is the only
//! plugin dispatcher.

use super::runner::Plugin;
use super::types::{PushResponse, SyncReport};
use crate::config::PluginEntry;
use crate::error::Result;
use crate::negotiation::{AttemptClass, FailurePolicy, Protocol};
use crate::participant::{Event, EventCtx, Participant, Projection};
use crate::store::Store;
use crate::task::Task;

/// One legacy plugin wrapped as a Participant. Owns its resolved
/// `Plugin` (paths and executable name); the per-event protocol
/// borrows it.
pub struct LegacyPluginParticipant {
    name: String,
    plugin: Plugin,
    projection: Projection,
    subscriptions: Vec<Event>,
    sync_filter: Option<String>,
}

impl LegacyPluginParticipant {
    /// Build the participant for an enabled plugin entry. `sync_filter`
    /// is the optional `--task` argument from `bl sync`; ignored for
    /// non-sync events.
    pub fn from_entry(
        store: &Store,
        name: String,
        entry: &PluginEntry,
        sync_filter: Option<String>,
    ) -> Self {
        let plugin = Plugin::resolve(store, &name, entry);
        let subscriptions = if entry.sync_on_change {
            vec![
                Event::Claim,
                Event::Review,
                Event::Close,
                Event::Update,
                Event::Sync,
            ]
        } else {
            vec![Event::Sync]
        };
        let projection = Projection::external_only(name.clone());
        Self {
            name,
            plugin,
            projection,
            subscriptions,
            sync_filter,
        }
    }
}

/// Outcome a legacy participant returns from a successful negotiation.
/// The variant is determined by the event the participant was
/// dispatched on; the dispatcher matches on it before applying.
#[derive(Debug)]
pub enum LegacyOutcome {
    Push(Option<PushResponse>),
    Sync(Option<SyncReport>),
}

/// Per-event Protocol state for legacy plugins. `Push` covers
/// claim/review/close/update; `Sync` covers the standalone sync event.
/// State is folded into the variants directly — the inner structs were
/// dead weight once both branches shared the same Protocol surface.
pub enum LegacyProtocol<'a> {
    Push {
        plugin: &'a Plugin,
        task: Box<Task>,
        response: Option<PushResponse>,
    },
    Sync {
        plugin: &'a Plugin,
        tasks: Vec<Task>,
        filter: Option<&'a str>,
        report: Option<SyncReport>,
    },
}

impl Protocol for LegacyProtocol<'_> {
    type Outcome = LegacyOutcome;

    fn propose(&mut self) -> Result<AttemptClass> {
        match self {
            LegacyProtocol::Push { plugin, task, response } => {
                if !plugin.auth_check() {
                    return Ok(AttemptClass::Other("auth check failed".into()));
                }
                // `Plugin::push` Err and `Ok(None)` are both treated as
                // "no usable response" today (the old dispatcher's
                // `if let Ok(Some(_))` swallowed Err). Collapsing them
                // into one arm preserves that and avoids an
                // effectively-unreachable Err branch.
                Ok(match plugin.push(task).ok().flatten() {
                    Some(r) => {
                        *response = Some(r);
                        AttemptClass::Ok
                    }
                    None => AttemptClass::Other("plugin push returned no data".into()),
                })
            }
            LegacyProtocol::Sync { plugin, tasks, filter, report } => {
                if !plugin.auth_check() {
                    return Ok(AttemptClass::Other("auth check failed".into()));
                }
                Ok(match plugin.sync(tasks, *filter).ok().flatten() {
                    Some(r) => {
                        *report = Some(r);
                        AttemptClass::Ok
                    }
                    None => AttemptClass::Other("plugin sync returned no data".into()),
                })
            }
        }
    }

    fn fetch_remote_view(&mut self) -> Result<()> {
        Ok(())
    }

    fn pushed(&mut self) -> Self::Outcome {
        match self {
            LegacyProtocol::Push { response, .. } => LegacyOutcome::Push(response.take()),
            LegacyProtocol::Sync { report, .. } => LegacyOutcome::Sync(report.take()),
        }
    }

    fn retry_budget(&self) -> usize {
        1
    }
}

impl Participant for LegacyPluginParticipant {
    type Outcome = LegacyOutcome;
    type Protocol<'a>
        = LegacyProtocol<'a>
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

    fn failure_policy(&self, _event: Event) -> FailurePolicy {
        // SPEC §12: legacy plugins are best-effort across the board.
        // Today's behavior is "swallow and warn"; BestEffort preserves
        // it via NegotiationResult::Skipped.
        FailurePolicy::BestEffort
    }

    fn protocol<'a>(
        &'a self,
        event: Event,
        ctx: EventCtx<'a>,
    ) -> Option<Self::Protocol<'a>> {
        match event {
            Event::Claim | Event::Review | Event::Close | Event::Update => {
                let task = ctx.store.load_task(ctx.task_id).ok()?;
                Some(LegacyProtocol::Push {
                    plugin: &self.plugin,
                    task: Box::new(task),
                    response: None,
                })
            }
            Event::Sync => {
                let tasks = ctx.store.all_tasks().ok()?;
                Some(LegacyProtocol::Sync {
                    plugin: &self.plugin,
                    tasks,
                    filter: self.sync_filter.as_deref(),
                    report: None,
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "participant_tests.rs"]
mod tests;
