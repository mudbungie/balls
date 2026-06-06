//! Tests for §12/§13 `prime`/`sync` orchestration. Chains run tracker-free
//! (`tracker_bin: None` ⇒ empty registry ⇒ no subprocess), so these exercise
//! the core logic — bootstrap, the pointer, trail walk, binding, flag parsing —
//! without a plugin binary; the end-to-end chain is `tests/dispatch.rs`.

use super::*;
use crate::edge::Edge;
use crate::git::run as git;
use crate::layout::Xdg;
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

/// The operating checkout this edge resolves to.
fn operating(e: &Edge) -> PathBuf {
    e.xdg.clone_dir(&e.invocation_path).operating()
}

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(ToString::to_string).collect()
}

#[test]
fn prime_founds_on_a_miss_then_converges_on_the_hit_path() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &argv(&["--as", "me"])).unwrap();
    assert!(operating(&e).join("config").join("balls.toml").is_file());
    // Re-prime: the landing already exists → hit path (rebind None), no error.
    prime(&e, &[]).unwrap();
}

#[test]
fn prime_center_writes_and_commits_the_trail_pointer() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &argv(&["--center", "git@hub:central"])).unwrap();
    let ptr = operating(&e).join("config/plugins/tracker/remote.toml");
    assert!(std::fs::read_to_string(&ptr).unwrap().contains("git@hub:central"));
    // Idempotent: the same center again changes nothing → commit_config no-ops.
    prime(&e, &argv(&["--center", "git@hub:central"])).unwrap();
    let log = git(&operating(&e), &["log", "--oneline"], None).unwrap();
    assert_eq!(log.matches("set trail pointer").count(), 1);
}

#[test]
fn prime_stealth_truncates_the_trail_pointer() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &argv(&["--center", "git@hub:central"])).unwrap();
    prime(&e, &argv(&["--stealth"])).unwrap();
    assert!(!operating(&e).join("config/plugins/tracker/remote.toml").exists());
}

#[test]
fn prime_honours_a_prior_stealth_lock_with_notice_w1() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    // Simulate a prior stealth prime having locked the landing.
    let bundle = e.xdg.clone_dir(&e.invocation_path);
    std::fs::write(bundle.root().join("stealth.lock"), "stealth\n").unwrap();
    // A plain re-prime must not auto-extend: it resolves no remote (W1).
    prime(&e, &[]).unwrap();
}

#[test]
fn prime_auto_discovers_the_origin_remote_when_unlocked() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    // Give the landing an origin; an unlocked, center-free prime discovers it.
    git(&operating(&e), &["remote", "add", "origin", "git@hub:origin"], None).unwrap();
    prime(&e, &[]).unwrap(); // resolves Some(origin) for the (empty) chain
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
    assert!(operating(&e).join("config/plugins/bin/tracker").symlink_metadata().is_ok());
}

#[test]
fn sync_before_prime_is_an_error() {
    let tmp = TempDir::new().unwrap();
    assert!(sync(&edge(&tmp, None), &[]).is_err());
}

#[test]
fn sync_walks_to_the_terminus_and_handles_named_branches() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    prime(&e, &[]).unwrap();
    sync(&e, &[]).unwrap(); // no arg: walk to terminus
    sync(&e, &argv(&["terminus"])).unwrap(); // the terminus alias
    sync(&e, &argv(&["work/bl-1234", "--as", "me"])).unwrap(); // a named branch
    sync(&e, &argv(&["landing"])).unwrap(); // the landing is never a target
}

#[test]
fn prime_rejects_conflicting_and_unknown_flags() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    assert!(prime(&e, &argv(&["--center", "u", "--stealth"])).is_err());
    assert!(prime(&e, &argv(&["--bogus"])).is_err());
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
