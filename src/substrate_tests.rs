//! Tests for §12 bootstrap-on-miss on throwaway repos — `found` makes a real
//! orphan `balls` checkout, with and without the default tracker wiring.

use super::*;
use crate::git::run as git;
use tempfile::TempDir;

/// A path under a fresh tempdir to found a landing at (the dir itself need not
/// pre-exist — `found` creates it).
fn operating(tmp: &TempDir) -> std::path::PathBuf {
    tmp.path().join("operating")
}

/// A standalone file standing in for an installed tracker binary.
fn fake_tracker(tmp: &TempDir) -> std::path::PathBuf {
    let bin = tmp.path().join("tracker");
    fs::write(&bin, "#!/bin/sh\n").unwrap();
    bin
}

#[test]
fn found_makes_an_orphan_balls_branch_with_seeded_config() {
    let tmp = TempDir::new().unwrap();
    let op = operating(&tmp);
    found(&op, None).unwrap();

    assert_eq!(git(&op, &["rev-parse", "--abbrev-ref", "HEAD"], None).unwrap().trim(), STATE_BRANCH);
    assert!(op.join("config").join("balls.toml").is_file());
    assert!(op.join(".gitignore").is_file());
    // The founding commit is the orphan branch's only (and root) commit.
    let log = git(&op, &["log", "--oneline"], None).unwrap();
    assert_eq!(log.lines().count(), 1);
    assert!(log.contains("balls: found"));
}

#[test]
fn found_without_a_tracker_lays_no_plugin_wiring() {
    let tmp = TempDir::new().unwrap();
    let op = operating(&tmp);
    found(&op, None).unwrap();
    // Empty = run nothing (§12): no plugins/ subtree at all.
    assert!(!op.join("config").join("plugins").exists());
}

#[test]
fn found_with_a_tracker_wires_and_binds_it() {
    let tmp = TempDir::new().unwrap();
    let op = operating(&tmp);
    found(&op, Some(&fake_tracker(&tmp))).unwrap();

    // Committed relative wiring: import slots + every deliverable verb's post.
    for slot in ["sync/pre/50-tracker", "prime/pre/50-tracker"] {
        assert!(op.join("config/plugins").join(slot).symlink_metadata().is_ok(), "{slot}");
    }
    for verb in ["create", "claim", "unclaim", "update", "close", "drop"] {
        let slot = format!("{verb}/post/90-tracker");
        assert!(op.join("config/plugins").join(&slot).symlink_metadata().is_ok(), "{slot}");
    }
    // The LOCAL bin/ binding exists but is gitignored — only the portable
    // relative wiring is tracked (§2).
    assert!(op.join("config/plugins/bin/tracker").symlink_metadata().is_ok());
    let tracked = git(&op, &["ls-files", "config/plugins"], None).unwrap();
    assert!(tracked.contains("sync/pre/50-tracker"));
    assert!(!tracked.contains("bin/tracker"), "bin/ must not be committed");
}
