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
        color: false,
        log_level: None,
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
fn a_named_sync_branch_overrides_the_config_tasks_branch_in_the_binding() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    let (l, s) = (landing(&e), store(&e));
    // No target ⇒ the config-named store branch; a target ⇒ that branch, which
    // is the one datum the tracker fetches/ff's (§13 `bl sync <branch>`).
    let (default_b, _) = bind(&e, &l, &s, None, None).unwrap();
    let (named_b, _) = bind(&e, &l, &s, None, Some("federation/shared".into())).unwrap();
    assert_eq!(named_b.tasks_branch, "federation/shared");
    assert_ne!(default_b.tasks_branch, named_b.tasks_branch);
}

#[test]
fn prime_rejects_unknown_flags_and_a_missing_value() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    assert!(prime(&e, &argv(&["--bogus"])).is_err());
    assert!(prime(&e, &argv(&["--as"])).is_err()); // flag with no value
    assert!(prime(&e, &argv(&["--remote"])).is_err()); // override flag with no value
    assert!(prime(&e, &argv(&["--center"])).is_err());
}

#[test]
fn prime_accepts_the_remote_override_flags() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    // --remote and --center both name the store remote; the empty (tracker-free)
    // chain ignores it, so this just proves they parse and resolve into the binding.
    prime(&e, &argv(&["--remote", "git@hub:r"])).unwrap();
    prime(&e, &argv(&["--center", "git@hub:c", "--remote", "git@hub:r"])).unwrap();
}

#[test]
fn resolve_remote_prefers_cli_then_xdg_then_origin() {
    let tmp = TempDir::new().unwrap();
    let landing = tmp.path().join("landing");
    std::fs::create_dir(&landing).unwrap();
    crate::git::run(&landing, &["init", "-q"], None).unwrap();
    crate::git::run(&landing, &["remote", "add", "origin", "git@hub:origin"], None).unwrap();
    let xdg = tmp.path().join("config.toml");
    std::fs::write(&xdg, "remote = \"git@hub:xdg\"\n").unwrap();

    // CLI override beats everything.
    assert_eq!(resolve_remote(Some("git@hub:cli".into()), &landing, &xdg).as_deref(), Some("git@hub:cli"));
    // No CLI → XDG beats origin.
    assert_eq!(resolve_remote(None, &landing, &xdg).as_deref(), Some("git@hub:xdg"));
    // No CLI, no XDG file → fall through to origin.
    let none = tmp.path().join("absent.toml");
    assert_eq!(resolve_remote(None, &landing, &none).as_deref(), Some("git@hub:origin"));
    // No CLI, no XDG, no origin → stealth (None).
    let bare = tmp.path().join("not-a-repo");
    std::fs::create_dir(&bare).unwrap();
    assert_eq!(resolve_remote(None, &bare, &none), None);
}

#[test]
fn sync_rejects_unknown_flags_and_a_second_branch() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    assert!(sync(&e, &argv(&["--bogus"])).is_err());
    assert!(sync(&e, &argv(&["a", "b"])).is_err());
}
