//! Tests for the pre-verb help/skill affordances (`skill`/`--skill`/`help`/
//! `--help`), the global `--log-level` / `-C` strip, and the usage/exit-code
//! conventions of [`crate::run`].

use super::support::*;
use super::{resolve_directory, strip_global, SKILL_DEPRECATION};
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
fn strip_global_pulls_a_flag_from_anywhere() {
    let s = |a: &[&str]| a.iter().map(ToString::to_string).collect::<Vec<_>>();
    // Leading the verb, with a value following.
    let (lvl, rest) = strip_global(&s(&["--log-level", "debug", "create", "X"]), "--log-level").unwrap();
    assert_eq!(lvl.as_deref(), Some("debug"));
    assert_eq!(rest, ["create", "X"]);
    // Mid-argv too — it is a global flag, position-independent.
    let (lvl, rest) = strip_global(&s(&["create", "--log-level", "error", "X"]), "--log-level").unwrap();
    assert_eq!(lvl.as_deref(), Some("error"));
    assert_eq!(rest, ["create", "X"]);
    // Absent ⇒ no override, argv untouched.
    let (lvl, rest) = strip_global(&s(&["list"]), "--log-level").unwrap();
    assert!(lvl.is_none());
    assert_eq!(rest, ["list"]);
    // Trailing with no value is a usage error.
    assert!(strip_global(&s(&["list", "--log-level"]), "--log-level").is_err());
    // The same lifting serves `-C` — one stripper, both globals.
    let (dir, rest) = strip_global(&s(&["list", "-C", "/proj"]), "-C").unwrap();
    assert_eq!(dir.as_deref(), Some("/proj"));
    assert_eq!(rest, ["list"]);
    assert!(strip_global(&s(&["-C"]), "-C").is_err());
}

#[test]
fn resolve_directory_canonicalizes_the_override_and_refuses_a_non_directory() {
    let tmp = TempDir::new().unwrap();
    let cwd = tmp.path().join("cwd");
    // Absent ⇒ the host cwd passes through untouched.
    assert_eq!(resolve_directory(None, &cwd).unwrap(), cwd);
    // Present ⇒ canonicalized (here: the `..` traversal collapses).
    let sub = tmp.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let indirect = sub.join("..").join("sub");
    let resolved = resolve_directory(Some(&indirect.to_string_lossy()), &cwd).unwrap();
    assert_eq!(resolved, std::fs::canonicalize(&sub).unwrap());
    // A path that does not exist, and a path that is a FILE, are both refused —
    // `-C` names a directory or nothing.
    let file = tmp.path().join("f.txt");
    std::fs::write(&file, "x").unwrap();
    for bad in [tmp.path().join("nope"), file] {
        let e = resolve_directory(Some(&bad.to_string_lossy()), &cwd).unwrap_err();
        assert!(e.contains("no such directory"), "balls-voice refusal, got {e}");
    }
}

#[test]
fn the_directory_override_addresses_the_store_keyed_by_that_path() {
    let tmp = TempDir::new().unwrap();
    // Found a substrate at the edge's own invocation path and file a ball there.
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    // A SECOND directory, primed only through `-C` — the flag is what addresses
    // it; the edge's cwd never changes.
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let there = |args: &[&str]| {
        let mut argv = vec!["-C".to_string(), elsewhere.to_string_lossy().into_owned()];
        argv.extend(args.iter().map(ToString::to_string));
        crate::run(&edge(&tmp), &argv)
    };
    assert_eq!(there(&["prime", "--as", "me"]), 0);
    assert_eq!(there(&["create", "Their task", "--as", "me"]), 0);
    // Two distinct stores: each holds exactly its own ball.
    let mine = sole_task_id(&store(&tmp).join("tasks"));
    let theirs = edge(&tmp).xdg.clone_dir(&std::fs::canonicalize(&elsewhere).unwrap()).store().join("tasks");
    assert_ne!(mine, sole_task_id(&theirs), "the -C store is a different store, not a view");
}

#[test]
fn a_directory_override_that_is_not_a_directory_is_a_usage_error() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["-C", &tmp.path().join("nope").to_string_lossy(), "list"]), 2);
    // A dangling `-C` (no value) is the same usage error, before any verb runs.
    assert_eq!(run_in(&tmp, &["-C"]), 2);
    // But help output still prints from a bad directory — it needs no substrate.
    assert_eq!(run_in(&tmp, &["-C", "/definitely/not/here", "--skill"]), 0);
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
