//! §6 self-describe and stderr-relay tests: `describe` parses a plugin's
//! `protocol` output (scalar and list versions, every error path) and
//! `capped_lines` bounds the relay so a no-newline flood cannot buffer whole.

use super::*;

const PROTO: &str =
    "if [ \"$1\" = protocol ]; then printf '%s' '{\"protocol\":1,\"ops\":[\"close\",\"claim\"]}'; exit 0; fi\n";

#[test]
fn describe_reads_a_scalar_protocol_version() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "p", PROTO);
    let p = describe(&bin).unwrap();
    assert_eq!(p.protocol, [1]);
    assert!(p.handles(Verb::Close));
    assert!(!p.handles(Verb::Sync));
    assert!(p.speaks(1));
}

#[test]
fn describe_reads_a_list_protocol_version() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "p", "printf '%s' '{\"protocol\":[1,2],\"ops\":[]}'\n");
    let p = describe(&bin).unwrap();
    assert_eq!(p.protocol, [1, 2]);
    assert!(p.speaks(2));
    assert!(!p.speaks(9));
    assert!(!p.handles(Verb::Close));
}

#[test]
fn describe_errors_on_a_nonzero_exit() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "p", "exit 1\n");
    let err = describe(&bin).unwrap_err();
    assert!(err.to_string().contains("self-describe exited"));
}

#[test]
fn describe_errors_on_unparseable_output() {
    let e = Env::new();
    let bin = script(&e.at("bin"), "p", "printf 'not json'\n");
    assert!(describe(&bin).is_err());
}

#[test]
fn describe_errors_when_the_binary_is_missing() {
    let e = Env::new();
    assert!(describe(&e.at("bin").join("nope")).is_err());
}

#[test]
fn capped_lines_splits_lines_and_trims_newlines() {
    // A newline-terminated stream and a final un-terminated blob both surface,
    // each with its trailing '\n' trimmed.
    let mut got = Vec::new();
    capped_lines(&b"alpha\nbeta\ngamma"[..], RELAY_LINE_MAX, |l| got.push(l.to_string()));
    assert_eq!(got, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn capped_lines_bounds_a_no_newline_flood() {
    // 10 KiB with no newline, cap 4 bytes: it is flushed in <=cap pieces rather
    // than buffered whole — the bl-2d6d OOM guard. Reassembled, no byte is lost.
    let flood = "x".repeat(10_240);
    let mut pieces = Vec::new();
    capped_lines(flood.as_bytes(), 4, |l| pieces.push(l.to_string()));
    assert!(pieces.iter().all(|p| p.len() <= 4), "every piece stays within the cap");
    assert_eq!(pieces.concat(), flood, "no byte dropped");
}
