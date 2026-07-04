//! Tests for the pre-verb help affordances (`skill`/`help`/`--help`), the
//! global `--log-level` strip, and the usage/exit-code conventions of
//! [`crate::run`].

use super::support::*;
use super::{strip_log_level, SKILL};
use tempfile::TempDir;

#[test]
fn skill_prints_the_guide_and_exits_zero() {
    // `skill` is a pre-verb help affordance: it needs no landing and is not a
    // Verb, so it works anywhere and never touches the store.
    assert_eq!(run_in(&TempDir::new().unwrap(), &["skill"]), 0);
    assert!(SKILL.contains("balls"), "the embedded guide is non-empty");
}

#[test]
fn help_prints_the_directory_and_exits_zero() {
    // `help` (and its conventional `--help`/`-h` aliases) is a pre-verb help
    // affordance like `skill`: no landing, not a Verb, works anywhere.
    for a in [&["help"][..], &["--help"], &["-h"]] {
        assert_eq!(run_in(&TempDir::new().unwrap(), a), 0);
    }
}

#[test]
fn per_command_help_routes_through_every_entry_point() {
    // bl-7990: `bl <cmd> --help`/`-h` is intercepted before the verb's parser, so
    // it needs no landing and no positionals; `bl help <cmd>` reaches the same
    // per-command help; `bl help <unknown>` falls back to the directory.
    let tmp = TempDir::new().unwrap();
    for a in [&["create", "--help"][..], &["create", "-h"], &["help", "update"], &["help", "frobnicate"]] {
        assert_eq!(run_in(&tmp, a), 0);
    }
}

#[test]
fn run_rejects_an_unknown_verb() {
    assert_eq!(run_in(&TempDir::new().unwrap(), &["frobnicate"]), 2);
}

#[test]
fn run_rejects_missing_verb() {
    assert_eq!(run_in(&TempDir::new().unwrap(), &[]), 2);
}

#[test]
fn strip_log_level_pulls_the_flag_from_anywhere() {
    let s = |a: &[&str]| a.iter().map(ToString::to_string).collect::<Vec<_>>();
    // Leading the verb, with a value following.
    let (lvl, rest) = strip_log_level(&s(&["--log-level", "debug", "create", "X"])).unwrap();
    assert_eq!(lvl.as_deref(), Some("debug"));
    assert_eq!(rest, ["create", "X"]);
    // Mid-argv too — it is a global flag, position-independent.
    let (lvl, rest) = strip_log_level(&s(&["create", "--log-level", "error", "X"])).unwrap();
    assert_eq!(lvl.as_deref(), Some("error"));
    assert_eq!(rest, ["create", "X"]);
    // Absent ⇒ no override, argv untouched.
    let (lvl, rest) = strip_log_level(&s(&["list"])).unwrap();
    assert!(lvl.is_none());
    assert_eq!(rest, ["list"]);
    // Trailing with no value is a usage error.
    assert!(strip_log_level(&s(&["list", "--log-level"])).is_err());
}

#[test]
fn a_dangling_log_level_flag_is_a_usage_error() {
    assert_eq!(run_in(&TempDir::new().unwrap(), &["--log-level"]), 2);
}

#[test]
fn the_log_level_override_threads_through_and_writes_the_op_log() {
    let tmp = TempDir::new().unwrap();
    // `--log-level debug` (layer 1) flows onto the edge and into both the diffless
    // (prime) and mutating (create) dispatch — the engine writes the op log.
    assert_eq!(run_in(&tmp, &["--log-level", "debug", "prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["--log-level", "debug", "create", "A task", "--as", "me"]), 0);
    let log = std::fs::read_to_string(op_log(&tmp)).unwrap();
    // Core's op-level lifecycle records land as JSON-lines (begin + seal).
    assert!(log.lines().any(|l| l.contains("\"msg\":\"begin\"")), "expected a begin record");
    assert!(log.lines().any(|l| l.contains("\"msg\":\"seal ")), "expected a seal record");
}
