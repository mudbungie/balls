//! Tests for the unified op log: the §4 threshold gate, the JSON-lines record
//! shape (op/phase stamping, absent-phase omission), and best-effort I/O.

use super::*;
use tempfile::TempDir;

/// A fixed clock so a record's `ts` is assertable.
fn clock() -> i64 {
    1_700_000_000
}

/// Build a `Log` writing to `<tmp>/log` at `threshold`, plus that path.
fn log_at(tmp: &TempDir, threshold: Level) -> (Log, std::path::PathBuf) {
    let path = tmp.path().join("log");
    (Log::new(path.clone(), threshold, Verb::Claim, clock), path)
}

/// The newline-delimited records written to `path` so far, each parsed as JSON.
fn lines(path: &std::path::Path) -> Vec<serde_json::Value> {
    let body = std::fs::read_to_string(path).unwrap_or_default();
    body.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[test]
fn a_record_at_threshold_lands_as_one_json_line() {
    let tmp = TempDir::new().unwrap();
    let (log, path) = log_at(&tmp, Level::Info);
    log.record(Level::Info, "core", Some(Phase::Pre), "invoke tracker");
    let recs = lines(&path);
    assert_eq!(recs.len(), 1);
    let r = &recs[0];
    assert_eq!(r["ts"], 1_700_000_000_i64);
    assert_eq!(r["lvl"], "info");
    assert_eq!(r["src"], "core");
    assert_eq!(r["op"], "claim");
    assert_eq!(r["phase"], "pre");
    assert_eq!(r["msg"], "invoke tracker");
}

#[test]
fn an_op_level_record_omits_the_phase_key() {
    let tmp = TempDir::new().unwrap();
    let (log, path) = log_at(&tmp, Level::Info);
    log.record(Level::Info, "core", None, "begin");
    let recs = lines(&path);
    assert!(recs[0].get("phase").is_none());
}

#[test]
fn a_below_threshold_record_is_emitted_nowhere() {
    let tmp = TempDir::new().unwrap();
    let (log, path) = log_at(&tmp, Level::Info);
    log.record(Level::Debug, "core", None, "narration");
    assert!(!path.exists()); // never opened
}

#[test]
fn an_error_record_outranks_a_raised_threshold() {
    let tmp = TempDir::new().unwrap();
    let (log, path) = log_at(&tmp, Level::Error);
    log.record(Level::Info, "tracker", Some(Phase::Post), "info chatter"); // dropped
    log.record(Level::Error, "core", Some(Phase::Post), "plugin tracker exited 1");
    let recs = lines(&path);
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0]["lvl"], "error");
}

#[test]
fn appends_accumulate_in_order() {
    let tmp = TempDir::new().unwrap();
    let (log, path) = log_at(&tmp, Level::Debug);
    log.record(Level::Debug, "core", None, "first");
    log.record(Level::Info, "core", None, "second");
    let recs = lines(&path);
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[0]["msg"], "first");
    assert_eq!(recs[1]["msg"], "second");
}

#[test]
fn an_unwritable_path_is_swallowed_not_panicked() {
    let tmp = TempDir::new().unwrap();
    // Parent dir does not exist ⇒ open fails; record must not panic.
    let path = tmp.path().join("missing").join("log");
    let log = Log::new(path.clone(), Level::Info, Verb::Close, clock);
    log.record(Level::Info, "core", None, "begin");
    assert!(!path.exists());
}

#[test]
fn level_parses_each_token_and_falls_back_to_info() {
    assert_eq!(Level::parse("debug"), Level::Debug);
    assert_eq!(Level::parse("info"), Level::Info);
    assert_eq!(Level::parse("error"), Level::Error);
    assert_eq!(Level::parse("garbage"), Level::Info);
}

#[test]
fn level_tokens_round_trip_and_order() {
    assert_eq!(Level::Debug.token(), "debug");
    assert_eq!(Level::Info.token(), "info");
    assert_eq!(Level::Error.token(), "error");
    assert!(Level::Debug < Level::Info && Level::Info < Level::Error);
}

#[test]
fn the_wall_clock_reads_a_recent_unix_time() {
    assert!(wall() >= 1_700_000_000); // well after 2023-11
}
