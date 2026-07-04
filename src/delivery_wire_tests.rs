//! Unit tests for the §7 wire slice the delivery plugin reads off stdin — that
//! the derived [`Wire`] deserializes the fields the plugin needs and tolerates
//! both a full close payload and a minimal pre one.

use super::*;

#[test]
fn wire_deserializes_the_slice_the_plugin_needs() {
    let json = r#"{
        "protocol": 1, "op": "close", "phase": "post", "plugin_name": "delivery",
        "actor": "me", "binding": {"branch": "balls", "store": "/s", "invocation_path": "/proj"},
        "command": {"op": "close", "message": "Full override [bl-f813]"},
        "current_state": {"title": "Refactor foo", "created": 0, "updated": 0},
        "metadata": {"bl-id": ["bl-f813"]}, "commit": "c", "previous_commit": "p"
    }"#;
    let wire: Wire = serde_json::from_str(json).unwrap();
    // The wire still carries `actor` (core writes it); the delivery slice no
    // longer reads it (bl-c2bf), so an unknown-to-us field is tolerated.
    assert_eq!(wire.binding.invocation_path, "/proj");
    assert_eq!(wire.current_state.unwrap().title, "Refactor foo");
    // The `-m` note rides the command for the delivery-message override (bl-b9a6).
    assert_eq!(wire.command.unwrap().message.as_deref(), Some("Full override [bl-f813]"));
    assert_eq!(wire.metadata.unwrap()["bl-id"], ["bl-f813"]);
    assert!(wire.rolling_back.is_none());
}

#[test]
fn wire_tolerates_a_minimal_pre_payload_and_a_rollback_tag() {
    let json = r#"{"binding": {"invocation_path": "/p"}, "rolling_back": "pre"}"#;
    let wire: Wire = serde_json::from_str(json).unwrap();
    assert_eq!(wire.rolling_back.as_deref(), Some("pre"));
    assert!(wire.metadata.is_none());
    assert!(wire.current_state.is_none());
    assert!(wire.command.is_none()); // no command on this slice → no override
}
