//! SPEC §5 "Config schema additions": the opt-in delivery/integration
//! fields (`state_remote`, `target_branch`, `delivery`,
//! `min_bl_version`). Each is `skip_serializing_if`-omitted when unset
//! so an untouched config stays byte-identical to before the field
//! existed, and each round-trips through save/load. Split out of
//! `config_tests.rs` to keep both files under the line cap.

use super::*;
use tempfile::TempDir;

#[test]
fn state_remote_default_is_origin() {
    // Unset field resolves to the historical hardcoded remote, so a
    // single-repo setup is byte-identical to before this field.
    let cfg = Config::default();
    assert_eq!(cfg.state_remote, None);
    assert_eq!(cfg.state_remote(), "origin");
    assert_eq!(cfg.state_remote(), DEFAULT_STATE_REMOTE);
}

#[test]
fn state_remote_none_is_omitted_from_serialization() {
    // skip_serializing_if keeps an unmodified config byte-identical:
    // the key must not appear when unset.
    let s = serde_json::to_string(&Config::default()).unwrap();
    assert!(
        !s.contains("state_remote"),
        "default config must not serialize state_remote: {s}"
    );
}

#[test]
fn state_remote_explicit_value_resolves_and_round_trips() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.json");
    let cfg = Config {
        state_remote: Some("taskhub".to_string()),
        ..Config::default()
    };
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.state_remote.as_deref(), Some("taskhub"));
    assert_eq!(loaded.state_remote(), "taskhub");
}

#[test]
fn target_branch_none_is_omitted_from_serialization() {
    // skip_serializing_if keeps an unmodified config byte-identical:
    // the key must not appear when unset, mirroring state_remote.
    let cfg = Config::default();
    assert_eq!(cfg.target_branch, None);
    let s = serde_json::to_string(&cfg).unwrap();
    assert!(
        !s.contains("target_branch"),
        "default config must not serialize target_branch: {s}"
    );
}

#[test]
fn target_branch_explicit_value_short_circuits_git_and_round_trips() {
    // A configured target_branch wins outright: `integration_branch`
    // returns it without consulting git, so a path that isn't even a
    // repo still resolves. This is the single-seam guarantee.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.json");
    let cfg = Config {
        target_branch: Some("develop".to_string()),
        ..Config::default()
    };
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.target_branch.as_deref(), Some("develop"));
    assert_eq!(
        loaded
            .integration_branch(std::path::Path::new("/no/such/repo"))
            .unwrap(),
        "develop"
    );
}

#[test]
fn min_bl_version_none_is_omitted_from_serialization() {
    // skip_serializing_if keeps an unmodified config byte-identical,
    // mirroring target_branch / delivery.
    let cfg = Config::default();
    assert_eq!(cfg.min_bl_version, None);
    let s = serde_json::to_string(&cfg).unwrap();
    assert!(
        !s.contains("min_bl_version"),
        "default config must not serialize min_bl_version: {s}"
    );
}

#[test]
fn min_bl_version_round_trips_through_load() {
    // Set value survives a save/load cycle; load still succeeds (the
    // advisory is a stderr nudge, never an error). 9.9.9 is below this
    // build so load also exercises the warn path without failing.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.json");
    let cfg = Config {
        min_bl_version: Some("9.9.9".to_string()),
        ..Config::default()
    };
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.min_bl_version.as_deref(), Some("9.9.9"));
}

#[test]
fn delivery_mode_defaults_and_round_trips() {
    // Unset delivery never serializes and resolves to local-squash.
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("d.json");
    let mut cfg = Config::default();
    cfg.save(&p).unwrap();
    assert!(!std::fs::read_to_string(&p).unwrap().contains("delivery"));
    assert_eq!(cfg.delivery_mode(), DeliveryMode::LocalSquash);
    cfg.delivery = Some(Delivery { mode: DeliveryMode::Deferred });
    cfg.save(&p).unwrap();
    assert!(std::fs::read_to_string(&p).unwrap().contains(r#""mode": "deferred""#));
    assert_eq!(Config::load(&p).unwrap().delivery_mode(), DeliveryMode::Deferred);
}

#[test]
fn review_pre_check_defaults_and_round_trips() {
    // Unset `review` never serializes and resolves to no gate (bl-1f38),
    // mirroring `delivery` / `target_branch`.
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("r.json");
    let mut cfg = Config::default();
    cfg.save(&p).unwrap();
    // The quoted key, so the substring can't match `require_remote_on_review`.
    assert!(!std::fs::read_to_string(&p).unwrap().contains(r#""review""#));
    assert_eq!(cfg.review_pre_check(), None);
    cfg.review = Some(ReviewConfig { pre_check: Some("make check".into()) });
    cfg.save(&p).unwrap();
    assert!(std::fs::read_to_string(&p).unwrap().contains(r#""pre_check": "make check""#));
    assert_eq!(Config::load(&p).unwrap().review_pre_check(), Some("make check"));
}
