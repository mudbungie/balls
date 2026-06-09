//! Tests for the §8 dispatch entrypoint [`crate::run`] — verb resolution,
//! the per-class wiring (prime/sync, mutate, reads), and exit-code conventions.

use super::*;
use std::path::Path;
use tempfile::TempDir;

/// An edge rooted in `tmp` with no plugin binaries installed (stealth) — prime
/// founds substrate, the seed prunes every default hook, and the chain runs
/// empty, so `run` needs no plugin subprocess.
fn edge(tmp: &TempDir) -> Edge {
    Edge {
        xdg: layout::Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "tester".into(),
        depth: 0,
        exe_dir: None,
        color: false,
        log_level: None,
    }
}

fn run_in(tmp: &TempDir, args: &[&str]) -> i32 {
    run(&edge(tmp), &args.iter().map(ToString::to_string).collect::<Vec<_>>())
}

#[test]
fn install_dispatches_to_its_run_wiring() {
    // The verb is wired (§6): before prime it reports the missing checkout
    // (exit 1, like any op), not a placeholder plan. The full seal path is
    // covered in `install_run_tests` / `tests/dispatch.rs`.
    assert_eq!(run_in(&TempDir::new().unwrap(), &["install", "--from", "balls/tasks"]), 1);
}

#[test]
fn a_read_verb_renders_the_store_and_exits_zero() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let id = sole_task_id(&store(&tmp).join("tasks"));
    // The reads dispatch through `reads::run` against the store (the old `ready`
    // verb is now `list --status ready`, §9).
    for a in [&["list"][..], &["list", "--status", "ready"], &["show", &id]] {
        assert_eq!(run_in(&tmp, a), 0);
    }
    // A read before prime is empty (§13); a missing id errors.
    assert_eq!(run_in(&TempDir::new().unwrap(), &["list"]), 0);
    assert_eq!(run_in(&tmp, &["show", "bl-nope"]), 1);
}

#[test]
fn a_mutating_verb_before_prime_is_an_error() {
    // No landing yet — a deliverable op never bootstraps, it reports the miss.
    assert_eq!(run_in(&TempDir::new().unwrap(), &["create", "A task"]), 1);
}

#[test]
fn create_claim_update_close_round_trip_through_the_engine() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    // create seals a fresh ball file onto the STORE.
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let tasks = store(&tmp).join("tasks");
    let id = sole_task_id(&tasks);
    assert_eq!(run_in(&tmp, &["claim", &id, "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["update", &id, "state=doing", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["close", &id, "--as", "me"]), 0);
    // close retires the file; the store has advanced past it.
    assert!(!tasks.join(format!("{id}.md")).exists());
}

/// The landing checkout for `tmp`'s edge.
fn landing(tmp: &TempDir) -> std::path::PathBuf {
    edge(tmp).xdg.clone_dir(Path::new(&edge(tmp).invocation_path)).landing()
}

/// The store checkout for `tmp`'s edge.
fn store(tmp: &TempDir) -> std::path::PathBuf {
    edge(tmp).xdg.clone_dir(Path::new(&edge(tmp).invocation_path)).store()
}

/// The single ball id under `tasks/` (basename minus `.md`).
fn sole_task_id(tasks: &Path) -> String {
    let mut ids: Vec<String> = std::fs::read_dir(tasks)
        .unwrap()
        .filter_map(|e| e.unwrap().file_name().to_string_lossy().strip_suffix(".md").map(str::to_string))
        .collect();
    assert_eq!(ids.len(), 1, "expected exactly one ball");
    ids.pop().unwrap()
}

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
fn run_rejects_an_unknown_verb() {
    assert_eq!(run_in(&TempDir::new().unwrap(), &["frobnicate"]), 2);
}

#[test]
fn run_rejects_missing_verb() {
    assert_eq!(run_in(&TempDir::new().unwrap(), &[]), 2);
}

#[test]
fn prime_founds_a_landing_then_converges_on_re_run() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert!(landing(&tmp).join("config").join("balls.toml").is_file());
    assert!(store(&tmp).join("tasks").is_dir());
    // Idempotent: a second prime is a no-op-converge, not an error (§12).
    assert_eq!(run_in(&tmp, &["prime"]), 0);
}

#[test]
fn sync_before_prime_is_an_error() {
    assert_eq!(run_in(&TempDir::new().unwrap(), &["sync"]), 1);
}

#[test]
fn sync_after_prime_targets_the_store() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime"]), 0);
    // Stealth store; the empty sync chain converges.
    assert_eq!(run_in(&tmp, &["sync"]), 0);
    assert_eq!(run_in(&tmp, &["sync", "landing"]), 0); // landing is never a target
}

#[test]
fn a_bad_flag_is_an_op_error() {
    assert_eq!(run_in(&TempDir::new().unwrap(), &["prime", "--center"]), 1);
}

/// The unified op log path for `tmp`'s edge.
fn op_log(tmp: &TempDir) -> std::path::PathBuf {
    edge(tmp).xdg.clone_dir(Path::new(&edge(tmp).invocation_path)).op_log()
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