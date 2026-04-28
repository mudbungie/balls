//! SPEC §11 — per-event participant policy with layered config
//! resolution.
//!
//! Layers, lowest precedence first:
//!
//! 1. Repo-default `.balls/config.json` (committed to the state branch).
//! 2. Per-clone `.balls/local/config.json` (gitignored).
//! 3. Per-invocation overrides (`--skip=NAME`, `--required=NAME`).
//!
//! The schema is additive on top of `PluginEntry`: a new optional
//! `participant.subscriptions` map declares per-event failure policy.
//! Configs that omit the new block fall through to the legacy
//! `sync_on_change` mapping, which produces byte-identical observable
//! behavior for unmodified plugins (SPEC §12).
//!
//! `effective_subscriptions` is what every dispatcher should call to
//! decide which events a plugin participates in and with what failure
//! policy. The legacy shim in `plugin::participant` calls it from
//! `LegacyPluginParticipant::from_entry`; bl-2bf7 wires per-invocation
//! overrides in from the CLI surface.
//!
//! Out of scope here: the human-gating staging mechanics behind
//! `PolicyKind::Gating` (bl-a46d) and the native plugin protocol that
//! lets plugins declare their own subscriptions (bl-8b71). The schema
//! is forward-compatible with both.

use crate::config::PluginEntry;
use crate::negotiation::FailurePolicy;
use crate::participant::Event;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The three failure-policy variants from SPEC §9, serialized as
/// `"required"`, `"best-effort"`, `"gating"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyKind {
    Required,
    BestEffort,
    Gating,
}

impl PolicyKind {
    pub fn into_failure_policy(self) -> FailurePolicy {
        match self {
            PolicyKind::Required => FailurePolicy::Required,
            PolicyKind::BestEffort => FailurePolicy::BestEffort,
            PolicyKind::Gating => FailurePolicy::Gating,
        }
    }
}

/// Per-event subscription policy. The struct shape leaves room for
/// future fields (per-event retry budgets, gate flags) without
/// breaking the on-disk schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventPolicy {
    pub policy: PolicyKind,
}

impl EventPolicy {
    pub fn new(policy: PolicyKind) -> Self {
        Self { policy }
    }
}

/// `participant` block inside a `PluginEntry`. The map keys are
/// lowercase event names; missing entries inherit from the next
/// higher-precedence layer (or, ultimately, the legacy mapping).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantConfig {
    #[serde(default)]
    pub subscriptions: BTreeMap<Event, EventPolicy>,
}

/// Per-clone override of a single plugin's participant config. Sits
/// inside `LocalConfig::plugins`. Only the fields a clone wants to
/// override are populated; everything else inherits from the
/// state-branch config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPluginEntry {
    #[serde(default)]
    pub participant: Option<ParticipantConfig>,
}

/// Per-invocation overrides. Populated from the CLI by callers in
/// bl-2bf7. The `skip` set removes a participant from the current
/// event's subscription set; `required` upgrades it to
/// `FailurePolicy::Required`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvocationOverrides {
    pub skip: BTreeSet<String>,
    pub required: BTreeSet<String>,
}

impl InvocationOverrides {
    pub fn is_empty(&self) -> bool {
        self.skip.is_empty() && self.required.is_empty()
    }
}

/// Legacy `sync_on_change` mapping (SPEC §11): `true` subscribes to
/// every push-shaped event with default `BestEffort`; `false`
/// subscribes only to the standalone `Sync` event. Either way the
/// resulting policies match today's swallow-and-warn behavior.
pub fn legacy_subscriptions(sync_on_change: bool) -> ParticipantConfig {
    let events: &[Event] = if sync_on_change {
        &[
            Event::Claim,
            Event::Review,
            Event::Close,
            Event::Update,
            Event::Sync,
        ]
    } else {
        &[Event::Sync]
    };
    let subscriptions = events
        .iter()
        .map(|ev| (*ev, EventPolicy::new(PolicyKind::BestEffort)))
        .collect();
    ParticipantConfig { subscriptions }
}

/// Resolve effective per-event subscriptions for one plugin, applying
/// the SPEC §11 layering: state-branch `participant` block (or legacy
/// `sync_on_change` when absent) → local override → per-invocation
/// `--skip` / `--required` flags.
///
/// Returns the events this plugin participates in for this invocation,
/// each mapped to the resolved `FailurePolicy`. An empty map means the
/// plugin is silent: callers should not invoke it at all.
pub fn effective_subscriptions(
    plugin_name: &str,
    repo_entry: &PluginEntry,
    local_override: Option<&LocalPluginEntry>,
    invocation: &InvocationOverrides,
) -> BTreeMap<Event, FailurePolicy> {
    if !repo_entry.enabled || invocation.skip.contains(plugin_name) {
        return BTreeMap::new();
    }
    let base = repo_entry
        .participant
        .clone()
        .unwrap_or_else(|| legacy_subscriptions(repo_entry.sync_on_change));
    let mut subs = base.subscriptions;
    if let Some(local_part) = local_override.and_then(|l| l.participant.as_ref()) {
        for (event, policy) in &local_part.subscriptions {
            subs.insert(*event, policy.clone());
        }
    }
    let mut out: BTreeMap<Event, FailurePolicy> = subs
        .into_iter()
        .map(|(ev, p)| (ev, p.policy.into_failure_policy()))
        .collect();
    if invocation.required.contains(plugin_name) {
        for policy in out.values_mut() {
            *policy = FailurePolicy::Required;
        }
    }
    out
}

#[cfg(test)]
#[path = "participant_config_tests.rs"]
mod tests;
