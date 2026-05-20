use super::{advisory, parse_triple, running_version, warn_if_below};

#[test]
fn running_version_is_a_well_formed_triple() {
    let v = running_version();
    assert_eq!(v, env!("CARGO_PKG_VERSION"));
    assert!(parse_triple(v).is_some(), "own version must parse: {v}");
}

#[test]
fn parse_triple_full_shorthand_and_malformed() {
    assert_eq!(parse_triple("0.3.10"), Some((0, 3, 10)));
    assert_eq!(parse_triple("0.4"), Some((0, 4, 0)));
    assert_eq!(parse_triple("1"), Some((1, 0, 0)));
    // Empty string splits to a single unparseable element.
    assert_eq!(parse_triple(""), None);
    // More than three components is rejected outright.
    assert_eq!(parse_triple("1.2.3.4"), None);
    // A non-numeric component fails the strict per-part parse.
    assert_eq!(parse_triple("1.x"), None);
}

#[test]
fn no_floor_configured_says_nothing() {
    assert_eq!(advisory(None, "0.3.10"), None);
}

#[test]
fn malformed_floor_reports_itself_ignored() {
    let msg = advisory(Some("nope"), "0.3.10").expect("malformed floor warns");
    assert!(msg.contains("not a valid"), "{msg}");
    assert!(msg.contains("\"nope\""), "echoes the bad value: {msg}");
    // Too-many-components is the other malformed shape.
    assert!(advisory(Some("1.2.3.4"), "0.3.10")
        .unwrap()
        .contains("not a valid"));
}

#[test]
fn unparseable_running_version_cannot_compare_so_is_silent() {
    assert_eq!(advisory(Some("1.0.0"), "not-a-version"), None);
}

#[test]
fn running_below_floor_warns_to_upgrade() {
    let msg = advisory(Some("9.9.9"), "0.3.10").expect("below floor warns");
    assert!(msg.contains("9.9.9"), "{msg}");
    assert!(msg.contains("0.3.10"), "{msg}");
    assert!(msg.contains("Upgrade bl"), "{msg}");
    assert!(msg.contains("advisory only"), "{msg}");
}

#[test]
fn running_at_or_above_floor_is_silent() {
    // Exactly at the floor: satisfied, no nudge.
    assert_eq!(advisory(Some("0.3.10"), "0.3.10"), None);
    // Comfortably above the floor: also silent.
    assert_eq!(advisory(Some("0.1.0"), "0.3.10"), None);
}

#[test]
fn warn_if_below_drives_both_io_branches() {
    // None ⇒ no advisory, no output. Some far-future floor ⇒ the
    // eprintln branch fires against the real running version. Neither
    // panics; this exercises the side-effecting wrapper end to end.
    warn_if_below(None);
    warn_if_below(Some("999.999.999"));
}
