//! Tests for the §8 dispatch entrypoint [`crate::run`] — verb resolution,
//! the per-class wiring (prime/sync, mutate, reads), and exit-code conventions.

use super::*;
use std::path::Path;
use tempfile::TempDir;

/// An edge rooted in `tmp` with no tracker installed (stealth) — prime founds
/// substrate and runs an empty chain, so `run` needs no plugin subprocess.
fn edge(tmp: &TempDir) -> Edge {
    Edge {
        xdg: layout::Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "tester".into(),
        depth: 0,
        tracker_bin: None,
        color: false,
    }
}

fn run_in(tmp: &TempDir, args: &[&str]) -> i32 {
    run(&edge(tmp), &args.iter().map(ToString::to_string).collect::<Vec<_>>())
}

#[test]
fn a_skeleton_verb_reports_its_op_plan() {
    // doctor is still unwired, so it prints its §8 op plan and exits 0.
    assert_eq!(run_in(&TempDir::new().unwrap(), &["doctor"]), 0);
}

#[test]
fn a_read_verb_renders_the_store_and_exits_zero() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(run_in(&tmp, &["prime", "--as", "me"]), 0);
    assert_eq!(run_in(&tmp, &["create", "A task", "--as", "me"]), 0);
    let id = sole_task_id(&store(&tmp).join("tasks"));
    // The four reads dispatch through `reads::run` against the store.
    for a in [&["list"][..], &["ready"], &["dep-tree"], &["show", &id]] {
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