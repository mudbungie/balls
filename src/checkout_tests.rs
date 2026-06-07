//! Tests for §12/§13 `prime`/`sync` orchestration. Chains run tracker-free
//! (`tracker_bin: None` ⇒ empty registry ⇒ no subprocess), so these exercise the
//! core logic — bootstrap of both branches, binding, flag parsing — without a
//! plugin binary; the end-to-end chain is `tests/dispatch.rs`.

use super::*;
use crate::edge::Edge;
use crate::layout::Xdg;
use std::path::PathBuf;
use tempfile::TempDir;

/// An edge rooted in `tmp` with the given (optional) installed tracker.
fn edge(tmp: &TempDir, tracker: Option<PathBuf>) -> Edge {
    Edge {
        xdg: Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "tester".into(),
        depth: 0,
        tracker_bin: tracker,
    }
}

/// The landing checkout this edge resolves to.
fn landing(e: &Edge) -> PathBuf {
    e.xdg.clone_dir(&e.invocation_path).landing()
}

/// The store checkout this edge resolves to.
fn store(e: &Edge) -> PathBuf {
    e.xdg.clone_dir(&e.invocation_path).store()
}

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(ToString::to_string).collect()
}

#[test]
fn prime_founds_both_branches_on_a_miss_then_converges_on_the_hit_path() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &argv(&["--as", "me"])).unwrap();
    assert!(landing(&e).join("config").join("balls.toml").is_file());
    assert!(store(&e).join("tasks").is_dir());
    // Re-prime: both checkouts already exist → hit path (rebind None), no error.
    prime(&e, &[]).unwrap();
}

#[test]
fn prime_auto_discovers_the_origin_remote_for_the_binding() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    // Give the landing an origin; a re-prime discovers it for the (empty) chain.
    crate::git::run(&landing(&e), &["remote", "add", "origin", "git@hub:origin"], None).unwrap();
    prime(&e, &[]).unwrap(); // resolves Some(origin) into the binding
}

#[test]
fn prime_rebinds_a_tracker_on_the_hit_path() {
    let tmp = TempDir::new().unwrap();
    // Found tracker-free, then re-prime with a tracker installed: rebind runs,
    // but with no committed wiring the chain stays empty (no subprocess).
    prime(&edge(&tmp, None), &[]).unwrap();
    let fake = tmp.path().join("tracker");
    std::fs::write(&fake, "x").unwrap();
    prime(&edge(&tmp, Some(fake)), &[]).unwrap();
    let e = edge(&tmp, None);
    assert!(landing(&e).join("config/plugins/bin/tracker").symlink_metadata().is_ok());
}

#[test]
fn sync_before_prime_is_an_error() {
    let tmp = TempDir::new().unwrap();
    assert!(sync(&edge(&tmp, None), &[]).is_err());
}

#[test]
fn sync_targets_the_store_and_treats_landing_as_a_no_op() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    sync(&e, &[]).unwrap(); // no arg: sync the store
    sync(&e, &argv(&["work/bl-1234", "--as", "me"])).unwrap(); // a named target
    sync(&e, &argv(&["landing"])).unwrap(); // the landing is never a target
}

#[test]
fn prime_rejects_unknown_flags_and_a_missing_value() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    assert!(prime(&e, &argv(&["--bogus"])).is_err());
    assert!(prime(&e, &argv(&["--center", "u"])).is_err()); // retired flag → unknown
    assert!(prime(&e, &argv(&["--as"])).is_err()); // flag with no value
}

#[test]
fn sync_rejects_unknown_flags_and_a_second_branch() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    assert!(sync(&e, &argv(&["--bogus"])).is_err());
    assert!(sync(&e, &argv(&["a", "b"])).is_err());
}
