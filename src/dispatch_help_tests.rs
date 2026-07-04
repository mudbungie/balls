//! Tests for the pre-verb help/skill affordances (`skill`/`--skill`/`help`/
//! `--help`), the global `--log-level` strip, and the usage/exit-code conventions
//! of [`crate::run`].

use super::support::*;
use super::{strip_log_level, SKILL_DEPRECATION};
use crate::verb::Verb;
use tempfile::TempDir;

#[test]
fn the_top_guide_prints_and_exits_zero_for_both_spellings() {
    // `bl --skill` (canonical) and `bl skill` (deprecated) both print the guide
    // and exit 0 — pre-verb, no landing, not a Verb, so they work anywhere.
    for a in [&["--skill"][..], &["skill"]] {
        assert_eq!(run_in(&TempDir::new().unwrap(), a), 0);
    }
    assert!(crate::skill::top().contains("balls"), "the embedded guide is non-empty");
}

#[test]
fn the_skill_subcommand_carries_a_deprecation_note() {
    // The `skill` subcommand is kept but on a deprecation path — the flag form
    // `bl --skill` is canonical, so the bare subcommand appends the migration note.
    assert!(SKILL_DEPRECATION.contains("DEPRECATION"), "the note names the path");
    assert!(SKILL_DEPRECATION.contains("bl --skill"), "the note names the replacement");
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
fn per_command_docs_route_through_every_entry_point() {
    // `bl <cmd> --skill`/`--help`/`-h` is intercepted before the verb's parser, so
    // it needs no landing and no positionals; `bl help <cmd>`, `bl skill <cmd>`,
    // and `bl --skill <cmd>` reach the SAME per-command doc — every verb, every
    // spelling — and `bl help <unknown>` falls back to the directory.
    let tmp = TempDir::new().unwrap();
    for v in Verb::ALL {
        let t = v.token();
        for a in [[t, "--skill"], [t, "--help"], [t, "-h"], ["help", t], ["skill", t], ["--skill", t]] {
            assert_eq!(run_in(&tmp, &a), 0, "{t} via {a:?}");
        }
    }
    assert_eq!(run_in(&tmp, &["help", "frobnicate"]), 0, "unknown falls back to the directory");
}

#[test]
fn per_command_help_is_folded_into_skill() {
    // The fold (this task): `bl <cmd> --help` and `bl <cmd> --skill` are one doc,
    // the same [`crate::skill::command`] the usage-error footer surfaces.
    for v in Verb::ALL {
        assert!(crate::skill::command(v).contains(v.token()), "{}'s doc names the verb", v.token());
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
