//! Unit tests for `plugin_policy`. `apply`/`describe` run against a
//! `Store::init`'d temp repo; `parse_*` and `apply_to_entry` are pure
//! and need no store.

use super::*;
use crate::config::Config;
use crate::git_test_support::init_repo;
use tempfile::tempdir;

fn standalone_store() -> (tempfile::TempDir, Store) {
    let td = tempdir().unwrap();
    init_repo(td.path());
    let store = Store::init(td.path(), false, None).unwrap();
    (td, store)
}

fn entry(participant: Option<ParticipantConfig>) -> PluginEntry {
    PluginEntry {
        enabled: true,
        sync_on_change: false,
        config_file: "p.json".into(),
        participant,
    }
}

fn block(events: &[(Event, PolicyKind)]) -> ParticipantConfig {
    let mut pc = ParticipantConfig::default();
    for (ev, kind) in events {
        pc.subscriptions.insert(*ev, EventPolicy::new(*kind));
    }
    pc
}

#[test]
fn event_name_round_trips_every_variant() {
    for ev in [
        Event::Claim,
        Event::Review,
        Event::Close,
        Event::Update,
        Event::Sync,
        Event::Create,
        Event::Drop,
    ] {
        assert_eq!(parse_event(event_name(ev)).unwrap(), ev);
    }
}

#[test]
fn kind_name_round_trips_every_variant() {
    for kind in [PolicyKind::Required, PolicyKind::BestEffort, PolicyKind::Gating] {
        assert_eq!(parse_kind(kind_name(kind)).unwrap(), kind);
    }
}

#[test]
fn parse_event_rejects_unknown_with_legal_set() {
    let err = parse_event("bogus").unwrap_err();
    assert!(matches!(err, BallError::Other(s)
        if s.contains("unknown event") && s.contains("create")));
}

#[test]
fn parse_kind_rejects_unknown_with_legal_set() {
    let err = parse_kind("loud").unwrap_err();
    assert!(matches!(err, BallError::Other(s)
        if s.contains("unknown policy") && s.contains("best-effort")));
}

#[test]
fn parse_set_token_splits_event_and_kind() {
    assert_eq!(
        parse_set_token("create=required").unwrap(),
        (Event::Create, PolicyKind::Required)
    );
}

#[test]
fn parse_set_token_rejects_missing_equals() {
    let err = parse_set_token("create").unwrap_err();
    assert!(matches!(err, BallError::Other(s) if s.contains("EVENT=KIND")));
}

#[test]
fn parse_set_token_propagates_bad_event_and_kind() {
    assert!(parse_set_token("bad=required").is_err());
    assert!(parse_set_token("create=bad").is_err());
}

#[test]
fn parse_op_picks_each_form() {
    assert_eq!(parse_op(&[], &[], true, false).unwrap(), PolicyOp::Clear);
    assert_eq!(parse_op(&[], &[], false, true).unwrap(), PolicyOp::NoLegacy);
    assert_eq!(
        parse_op(&[], &["close".into()], false, false).unwrap(),
        PolicyOp::Rm(vec![Event::Close])
    );
    assert_eq!(
        parse_op(&["sync=gating".into()], &[], false, false).unwrap(),
        PolicyOp::Set(vec![(Event::Sync, PolicyKind::Gating)])
    );
}

#[test]
fn parse_op_rejects_empty_and_bad_tokens() {
    let err = parse_op(&[], &[], false, false).unwrap_err();
    assert!(matches!(err, BallError::Other(s) if s.contains("nothing to do")));
    assert!(parse_op(&[], &["nope".into()], false, false).is_err());
    assert!(parse_op(&["create=nope".into()], &[], false, false).is_err());
}

#[test]
fn apply_to_entry_clear_drops_block() {
    let mut e = entry(Some(block(&[(Event::Create, PolicyKind::BestEffort)])));
    apply_to_entry(&mut e, PolicyOp::Clear).unwrap();
    assert!(e.participant.is_none());
}

#[test]
fn apply_to_entry_no_legacy_writes_explicit_empty() {
    let mut e = entry(None);
    apply_to_entry(&mut e, PolicyOp::NoLegacy).unwrap();
    assert_eq!(e.participant, Some(ParticipantConfig::default()));
}

#[test]
fn apply_to_entry_set_materializes_and_upserts() {
    let mut e = entry(None);
    apply_to_entry(&mut e, PolicyOp::Set(vec![(Event::Create, PolicyKind::Required)])).unwrap();
    apply_to_entry(
        &mut e,
        PolicyOp::Set(vec![(Event::Create, PolicyKind::Gating), (Event::Sync, PolicyKind::BestEffort)]),
    )
    .unwrap();
    let subs = &e.participant.unwrap().subscriptions;
    assert_eq!(subs[&Event::Create].policy, PolicyKind::Gating);
    assert_eq!(subs[&Event::Sync].policy, PolicyKind::BestEffort);
}

#[test]
fn apply_to_entry_rm_leaves_empty_block_not_none() {
    let mut e = entry(Some(block(&[(Event::Close, PolicyKind::BestEffort)])));
    apply_to_entry(&mut e, PolicyOp::Rm(vec![Event::Close])).unwrap();
    // Dropping the last subscription leaves an explicit `{}`, not
    // None — only `--clear` returns to the legacy fallback.
    assert_eq!(e.participant, Some(ParticipantConfig::default()));
}

#[test]
fn apply_to_entry_rm_on_legacy_block_errors() {
    let mut e = entry(None);
    let err = apply_to_entry(&mut e, PolicyOp::Rm(vec![Event::Close])).unwrap_err();
    assert!(matches!(err, BallError::Other(s) if s.contains("no participant block")));
}

#[test]
fn apply_rejects_invalid_name_and_unknown_plugin() {
    let (_td, store) = standalone_store();
    assert!(apply(&store, "../bad", PolicyOp::Clear).is_err());
    let err = apply(&store, "ghost", PolicyOp::Clear).unwrap_err();
    assert!(matches!(err, BallError::Other(s) if s.contains("no plugin named")));
}

#[test]
fn apply_persists_policy_to_effective_config() {
    let (_td, store) = standalone_store();
    plugin_admin::enable(&store, "watcher", None, false).unwrap();
    apply(
        &store,
        "watcher",
        PolicyOp::Set(vec![(Event::Create, PolicyKind::Required)]),
    )
    .unwrap();

    let cfg = Config::load(&store.config_path()).unwrap();
    let subs = &cfg.plugins["watcher"].participant.as_ref().unwrap().subscriptions;
    assert_eq!(subs[&Event::Create].policy, PolicyKind::Required);
}

#[test]
fn apply_rejects_drop_with_non_best_effort_policy() {
    let (_td, store) = standalone_store();
    plugin_admin::enable(&store, "watcher", None, false).unwrap();
    let err = apply(
        &store,
        "watcher",
        PolicyOp::Set(vec![(Event::Drop, PolicyKind::Required)]),
    )
    .unwrap_err();
    assert!(matches!(err, BallError::Other(s) if s.contains("observe-only")));
    // Validation runs before save — the rejected block never lands.
    let cfg = Config::load(&store.config_path()).unwrap();
    assert!(cfg.plugins["watcher"].participant.is_none());
}

#[test]
fn describe_rejects_invalid_name_and_unknown_plugin() {
    let (_td, store) = standalone_store();
    assert!(describe(&store, "../bad").is_err());
    assert!(describe(&store, "ghost").is_err());
}

#[test]
fn describe_reports_explicit_block() {
    let (_td, store) = standalone_store();
    plugin_admin::enable(&store, "watcher", None, false).unwrap();
    apply(
        &store,
        "watcher",
        PolicyOp::Set(vec![(Event::Update, PolicyKind::Gating)]),
    )
    .unwrap();
    let view = describe(&store, "watcher").unwrap();
    assert!(view.explicit);
    assert_eq!(view.resolved.subscriptions[&Event::Update].policy, PolicyKind::Gating);
}

#[test]
fn describe_falls_back_to_legacy_when_no_block() {
    let (_td, store) = standalone_store();
    plugin_admin::enable(&store, "watcher", None, true).unwrap();
    let view = describe(&store, "watcher").unwrap();
    assert!(!view.explicit);
    // sync_on_change=true legacy projection subscribes to `close`.
    assert!(view.resolved.subscriptions.contains_key(&Event::Close));
}
