//! §6 primitives beneath the dispatch: [`describe`] (the `protocol` self-describe
//! a binary answers at install time) and [`capped_lines`] (the bounded stderr
//! relay that envelopes a plugin's log stream). Neither spawns a real op, so they
//! need only a throwaway script, not the dispatcher harness in `plugin_tests`.

#![cfg(unix)]

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Write an executable `#!/bin/sh` plugin into `dir` and return its path.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

const PROTO: &str =
    "if [ \"$1\" = protocol ]; then printf '%s' '{\"protocol\":1,\"ops\":[\"close\",\"claim\"]}'; exit 0; fi\n";

#[test]
fn describe_reads_a_scalar_protocol_version() {
    let dir = TempDir::new().unwrap();
    let bin = script(dir.path(), "p", PROTO);
    let p = describe(&bin).unwrap();
    assert_eq!(p.protocol, [1]);
    assert_eq!(p.ops, ["close", "claim"]);
    assert!(p.speaks(1));
}

#[test]
fn describe_reads_a_list_protocol_version() {
    let dir = TempDir::new().unwrap();
    let bin = script(dir.path(), "p", "printf '%s' '{\"protocol\":[1,2],\"ops\":[]}'\n");
    let p = describe(&bin).unwrap();
    assert_eq!(p.protocol, [1, 2]);
    assert!(p.speaks(2));
    assert!(!p.speaks(9));
    assert!(p.ops.is_empty());
}

#[test]
fn describe_errors_on_a_nonzero_exit() {
    let dir = TempDir::new().unwrap();
    let bin = script(dir.path(), "p", "exit 1\n");
    let err = describe(&bin).unwrap_err();
    assert!(err.to_string().contains("self-describe exited"));
}

#[test]
fn describe_errors_on_unparseable_output() {
    let dir = TempDir::new().unwrap();
    let bin = script(dir.path(), "p", "printf 'not json'\n");
    assert!(describe(&bin).is_err());
}

#[test]
fn describe_errors_when_the_binary_is_missing() {
    let dir = TempDir::new().unwrap();
    assert!(describe(&dir.path().join("nope")).is_err());
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
