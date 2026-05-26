//! SPEC §11 — per-event participant policy with layered config
//! resolution.
//!
//! Layers, lowest precedence first:
//!
//! 1. Project-default `project.json` (committed to the tracker branch
//!    — the plugin map is project config, SPEC §7).
//! 2. Per-clone override — historically `.balls/local/config.json`,
//!    retired by bl-5a03 because SPEC §6.5 rejects `plugins` in
//!    `clone.json`. The layering shape stays in place for the
//!    in-memory contract; today the source is always `None` in
//!    production.
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
//! `PolicyKind::Gating` (deferred per bl-6969 — see `into_failure_policy`
//! for the runtime degrade to `Required`) and the native plugin
//! protocol that lets plugins declare their own subscriptions
//! (bl-8b71). The schema is forward-compatible with both.

use crate::config::PluginEntry;
use crate::negotiation::FailurePolicy;
use crate::participant::Event;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

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
    /// Resolve a configured policy to its runtime `FailurePolicy`.
    /// `Gating` degrades to `Required` with a once-per-process warning
    /// while the staging surface (`bl sync --review/--apply/--discard`)
    /// is deferred (bl-6969). `Required` is the closest fail-loud
    /// approximation; `BestEffort` would silently swallow failures the
    /// operator wanted to gate on.
    pub fn into_failure_policy(self) -> FailurePolicy {
        match self {
            PolicyKind::Required => FailurePolicy::Required,
            PolicyKind::BestEffort => FailurePolicy::BestEffort,
            PolicyKind::Gating => {
                warn_gating_deferred();
                FailurePolicy::Required
            }
        }
    }
}

static GATING_WARNED: OnceLock<()> = OnceLock::new();

/// Emit the gating-deferred warning at most once per process.
fn warn_gating_deferred() {
    GATING_WARNED.get_or_init(|| {
        eprintln!(
            "warning: plugin policy \"gating\" is configured but staging is deferred (see bl-6969); treating as \"required\" for this invocation"
        );
    });
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

/// Per-clone override of a single plugin's participant config. SPEC
/// §6.5 rejects `plugins` in `clone.json` (the field is tracker-
/// scope), so today this type has no per-clone source — it survives
/// as the in-memory contract `effective_subscriptions` accepts, used
/// by tests that exercise the layering math directly and reserved
/// for whatever future SPEC revision reopens per-clone plugin
/// overrides. Production dispatchers pass `None`.
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

/// SPEC §11 / §5.1 — the per-invocation override tokens applied to
/// this event, in a stable order (git-remote sync flag, then sorted
/// `--skip`, then sorted `--required`). Used verbatim in the EventCtx
/// `overrides` array and, bracket-wrapped by [`override_log`],
/// appended to the state-branch commit message so a post-hoc audit
/// sees which negotiations ran without their required participants.
/// `BTreeSet` iteration is already sorted, so the result is
/// deterministic across runs — load-bearing for commit-message tests.
pub fn override_tokens(ov: &InvocationOverrides, sync: bool, no_sync: bool) -> Vec<String> {
    let mut out = Vec::new();
    if sync {
        out.push("--sync".to_string());
    } else if no_sync {
        out.push("--no-sync".to_string());
    }
    out.extend(ov.skip.iter().map(|n| format!("--skip={n}")));
    out.extend(ov.required.iter().map(|n| format!("--required={n}")));
    out
}

/// The §11 audit fragment for a state-branch commit subject: each
/// token from [`override_tokens`] wrapped in brackets and space-
/// joined, with a leading space so it appends cleanly. Empty (and
/// thus a no-op append) when no override applied — keeping default
/// invocations byte-identical per SPEC §12.
pub fn override_log(tokens: &[String]) -> String {
    if tokens.is_empty() {
        return String::new();
    }
    let joined = tokens
        .iter()
        .map(|t| format!("[{t}]"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(" {joined}")
}

/// Legacy `sync_on_change` mapping (SPEC §11): `true` subscribes to
/// every push-shaped event with default `BestEffort`; `false`
/// subscribes only to the standalone `Sync` event. Either way the
/// resulting policies match today's swallow-and-warn behavior.
pub fn legacy_subscriptions(sync_on_change: bool) -> ParticipantConfig {
    // `Create` is included so a legacy plugin still fires on
    // `bl create` exactly as before bl-ec62 (when creation rode
    // `Update`): the legacy push wire carries no event name, so the
    // observable behavior is byte-identical (SPEC §12 / §17.1).
    // `Drop` is deliberately omitted: `bl drop` was silent for legacy
    // plugins and must stay so — drop is native-only/observe-only
    // (SPEC §6.2).
    let events: &[Event] = if sync_on_change {
        &[
            Event::Create,
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
