//! Tests for SPEC §11 layered resolution: legacy round-trip, layered
//! override, per-invocation `--skip` / `--required` shadowing.

use super::*;
use crate::config::{Config, PluginEntry};
use crate::participant::Event;

fn legacy_entry(sync_on_change: bool) -> PluginEntry {
    PluginEntry {
        enabled: true,
        sync_on_change,
        config_file: ".balls/plugins/x.json".into(),
        participant: None,
    }
}

fn entry_with(participant: ParticipantConfig) -> PluginEntry {
    PluginEntry {
        enabled: true,
        sync_on_change: false,
        config_file: ".balls/plugins/x.json".into(),
        participant: Some(participant),
    }
}

fn one_event(event: Event, kind: PolicyKind) -> ParticipantConfig {
    let mut subs = BTreeMap::new();
    subs.insert(event, EventPolicy::new(kind));
    ParticipantConfig { subscriptions: subs }
}

#[test]
fn policy_kind_into_failure_policy_round_trip() {
    assert_eq!(
        PolicyKind::Required.into_failure_policy(),
        FailurePolicy::Required
    );
    assert_eq!(
        PolicyKind::BestEffort.into_failure_policy(),
        FailurePolicy::BestEffort
    );
    assert_eq!(
        PolicyKind::Gating.into_failure_policy(),
        FailurePolicy::Gating
    );
}

#[test]
fn policy_kind_serializes_kebab_case() {
    let s = serde_json::to_string(&PolicyKind::BestEffort).unwrap();
    assert_eq!(s, "\"best-effort\"");
    let back: PolicyKind = serde_json::from_str(&s).unwrap();
    assert_eq!(back, PolicyKind::BestEffort);
}

#[test]
fn event_serializes_lowercase_for_map_key() {
    let mut subs = BTreeMap::new();
    subs.insert(Event::Claim, EventPolicy::new(PolicyKind::Required));
    let cfg = ParticipantConfig { subscriptions: subs };
    let s = serde_json::to_string(&cfg).unwrap();
    assert!(s.contains("\"claim\""), "got {s}");
    let back: ParticipantConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(back, cfg);
}

#[test]
fn legacy_subscriptions_true_covers_every_event() {
    let cfg = legacy_subscriptions(true);
    for ev in [
        Event::Claim,
        Event::Review,
        Event::Close,
        Event::Update,
        Event::Sync,
    ] {
        assert_eq!(cfg.subscriptions.get(&ev).map(|p| p.policy), Some(PolicyKind::BestEffort));
    }
}

#[test]
fn legacy_subscriptions_false_is_sync_only() {
    let cfg = legacy_subscriptions(false);
    assert_eq!(cfg.subscriptions.len(), 1);
    assert!(cfg.subscriptions.contains_key(&Event::Sync));
}

#[test]
fn legacy_round_trip_reproduces_today_for_sync_on_change_true() {
    // SPEC §12 byte-identical contract: a legacy entry without the
    // `participant` block must yield the same effective subscription
    // set as today's hard-coded sync_on_change=true branch.
    let entry = legacy_entry(true);
    let resolved = effective_subscriptions(
        "legacy",
        &entry,
        None,
        &InvocationOverrides::default(),
    );
    for ev in [
        Event::Claim,
        Event::Review,
        Event::Close,
        Event::Update,
        Event::Sync,
    ] {
        assert_eq!(resolved.get(&ev), Some(&FailurePolicy::BestEffort));
    }
}

#[test]
fn legacy_round_trip_reproduces_today_for_sync_on_change_false() {
    let entry = legacy_entry(false);
    let resolved = effective_subscriptions(
        "legacy",
        &entry,
        None,
        &InvocationOverrides::default(),
    );
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved.get(&Event::Sync), Some(&FailurePolicy::BestEffort));
}

#[test]
fn layered_override_local_replaces_required_with_best_effort() {
    // State-branch declares close=required; per-clone disagrees and
    // sets close=best-effort. The clone wins.
    let entry = entry_with(one_event(Event::Close, PolicyKind::Required));
    let local = LocalPluginEntry {
        participant: Some(one_event(Event::Close, PolicyKind::BestEffort)),
    };
    let resolved = effective_subscriptions(
        "x",
        &entry,
        Some(&local),
        &InvocationOverrides::default(),
    );
    assert_eq!(resolved.get(&Event::Close), Some(&FailurePolicy::BestEffort));
}

#[test]
fn layered_override_inherits_unspecified_events_from_repo() {
    // Repo declares both claim=required and close=best-effort. Local
    // only overrides close. Claim must inherit from the state branch.
    let mut repo_subs = BTreeMap::new();
    repo_subs.insert(Event::Claim, EventPolicy::new(PolicyKind::Required));
    repo_subs.insert(Event::Close, EventPolicy::new(PolicyKind::BestEffort));
    let entry = entry_with(ParticipantConfig { subscriptions: repo_subs });
    let local = LocalPluginEntry {
        participant: Some(one_event(Event::Close, PolicyKind::Gating)),
    };
    let resolved = effective_subscriptions(
        "x",
        &entry,
        Some(&local),
        &InvocationOverrides::default(),
    );
    assert_eq!(resolved.get(&Event::Claim), Some(&FailurePolicy::Required));
    assert_eq!(resolved.get(&Event::Close), Some(&FailurePolicy::Gating));
}

#[test]
fn invocation_skip_drops_plugin_entirely() {
    let entry = legacy_entry(true);
    let mut overrides = InvocationOverrides::default();
    overrides.skip.insert("x".into());
    let resolved = effective_subscriptions("x", &entry, None, &overrides);
    assert!(resolved.is_empty());
    assert!(!overrides.is_empty());
}

#[test]
fn invocation_required_upgrades_every_event() {
    // Per-invocation --required=NAME shadows both repo and local; the
    // SPEC §11 row mirrors --sync for git-remote but at the plugin
    // level. Every subscribed event is forced to Required.
    let entry = legacy_entry(true);
    let mut overrides = InvocationOverrides::default();
    overrides.required.insert("x".into());
    let resolved = effective_subscriptions("x", &entry, None, &overrides);
    for policy in resolved.values() {
        assert_eq!(*policy, FailurePolicy::Required);
    }
}

#[test]
fn disabled_repo_entry_yields_no_subscriptions() {
    // Master switch (`enabled: false`) wins regardless of local or
    // invocation overrides. Mirrors today's
    // `cfg.plugins.iter().filter(|(_, e)| e.enabled)` filter.
    let mut entry = legacy_entry(true);
    entry.enabled = false;
    let mut overrides = InvocationOverrides::default();
    overrides.required.insert("x".into());
    let resolved = effective_subscriptions("x", &entry, None, &overrides);
    assert!(resolved.is_empty());
}

#[test]
fn invocation_overrides_default_is_empty() {
    let o = InvocationOverrides::default();
    assert!(o.is_empty());
}

#[test]
fn participant_config_round_trip_through_full_config() {
    // Full Config carrying a participant block round-trips through
    // serde_json without losing the per-event policy.
    let mut cfg = Config::default();
    cfg.plugins.insert(
        "jira".into(),
        entry_with(one_event(Event::Close, PolicyKind::Required)),
    );
    let s = serde_json::to_string(&cfg).unwrap();
    assert!(s.contains("\"participant\""));
    assert!(s.contains("\"close\""));
    let back: Config = serde_json::from_str(&s).unwrap();
    let plug = back.plugins.get("jira").unwrap();
    let part = plug.participant.as_ref().unwrap();
    assert_eq!(
        part.subscriptions.get(&Event::Close).map(|p| p.policy),
        Some(PolicyKind::Required)
    );
}

#[test]
fn legacy_config_without_participant_block_round_trips() {
    // Configs written before this ball must continue to deserialize:
    // no `participant` key, only the legacy fields.
    let s = r#"{
        "version": 1,
        "id_length": 4,
        "stale_threshold_seconds": 60,
        "worktree_dir": ".balls-worktrees",
        "plugins": {
            "jira": {
                "enabled": true,
                "sync_on_change": true,
                "config_file": ".balls/plugins/jira.json"
            }
        }
    }"#;
    let cfg: Config = serde_json::from_str(s).unwrap();
    let entry = cfg.plugins.get("jira").unwrap();
    assert!(entry.participant.is_none());
    let resolved = effective_subscriptions(
        "jira",
        entry,
        None,
        &InvocationOverrides::default(),
    );
    assert_eq!(resolved.len(), 5);
}
