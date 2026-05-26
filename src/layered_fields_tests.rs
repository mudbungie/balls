use super::*;

#[test]
fn integrate_mode_default_is_direct() {
    assert_eq!(IntegrateMode::default(), IntegrateMode::Direct);
}

#[test]
fn integrate_mode_serializes_kebab_case() {
    let s = serde_json::to_string(&IntegrateMode::Direct).unwrap();
    assert_eq!(s, "\"direct\"");
    let s = serde_json::to_string(&IntegrateMode::ForgePr).unwrap();
    assert_eq!(s, "\"forge-pr\"");
}

#[test]
fn integrate_mode_round_trips() {
    for m in [IntegrateMode::Direct, IntegrateMode::ForgePr] {
        let s = serde_json::to_string(&m).unwrap();
        let parsed: IntegrateMode = serde_json::from_str(&s).unwrap();
        assert_eq!(m, parsed);
    }
}

#[test]
fn integrate_mode_rejects_legacy_local_squash() {
    // SPEC §6.6 / §14.16: the legacy `local-squash` value is the
    // old `delivery.mode` shape. New schema must NOT accept it —
    // dual-read handles legacy as a separate path with rename in
    // memory.
    let r: serde_json::Result<IntegrateMode> = serde_json::from_str("\"local-squash\"");
    assert!(r.is_err());
}

#[test]
fn integrate_mode_rejects_legacy_deferred() {
    let r: serde_json::Result<IntegrateMode> = serde_json::from_str("\"deferred\"");
    assert!(r.is_err());
}

#[test]
fn integrate_block_default_is_direct_mode() {
    let i = Integrate::default();
    assert_eq!(i.mode, IntegrateMode::Direct);
}

#[test]
fn integrate_block_deny_unknown_field() {
    let r: serde_json::Result<Integrate> =
        serde_json::from_str(r#"{"mode": "direct", "extra": true}"#);
    assert!(r.is_err());
}

#[test]
fn integrate_block_round_trips() {
    let i = Integrate {
        mode: IntegrateMode::ForgePr,
    };
    let s = serde_json::to_string(&i).unwrap();
    let parsed: Integrate = serde_json::from_str(&s).unwrap();
    assert_eq!(i, parsed);
}

#[test]
fn review_block_default_has_no_gate_command() {
    let r = ReviewBlock::default();
    assert_eq!(r.gate_command, None);
}

#[test]
fn review_block_serializes_with_some_gate_command() {
    let r = ReviewBlock {
        gate_command: Some("make check".into()),
    };
    let s = serde_json::to_string(&r).unwrap();
    assert!(s.contains("gate_command"));
    assert!(s.contains("make check"));
}

#[test]
fn review_block_skips_none_gate_command_in_output() {
    let r = ReviewBlock {
        gate_command: None,
    };
    let s = serde_json::to_string(&r).unwrap();
    assert!(!s.contains("gate_command"), "{s}");
}

#[test]
fn review_block_rejects_legacy_pre_check_field() {
    // SPEC §14.17: a `pre_check` field on a freshly-written
    // `review` block aborts read (rename is enforced). Legacy
    // dual-read handles the rename in memory.
    let r: serde_json::Result<ReviewBlock> =
        serde_json::from_str(r#"{"pre_check": "make check"}"#);
    assert!(r.is_err());
}

#[test]
fn review_block_round_trips() {
    for cmd in [None, Some("make check".to_string())] {
        let r = ReviewBlock {
            gate_command: cmd.clone(),
        };
        let s = serde_json::to_string(&r).unwrap();
        let parsed: ReviewBlock = serde_json::from_str(&s).unwrap();
        assert_eq!(r, parsed);
    }
}
