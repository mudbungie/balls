//! Tests for §12 bootstrap-on-miss on throwaway repos — `found` makes BOTH
//! branches of the two-branch substrate (the `balls/config` landing + the
//! `balls/tasks` store), with and without the default tracker wiring.

use super::*;
use crate::git::run as git;
use tempfile::TempDir;

/// The two checkout paths under a fresh tempdir (neither need pre-exist — `found`
/// creates the landing repo and adds the store as a linked worktree).
fn paths(tmp: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    (tmp.path().join("config"), tmp.path().join("tasks"))
}

/// A standalone file standing in for an installed tracker binary.
fn fake_tracker(tmp: &TempDir) -> std::path::PathBuf {
    let bin = tmp.path().join("tracker");
    fs::write(&bin, "#!/bin/sh\n").unwrap();
    bin
}

#[test]
fn found_makes_both_branches_with_seeded_config_and_store() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found(&landing, &store, None).unwrap();

    // The landing is the balls/config branch with a seeded config/.
    assert_eq!(git(&landing, &["rev-parse", "--abbrev-ref", "HEAD"], None).unwrap().trim(), LANDING_BRANCH);
    assert!(landing.join("config").join("balls.toml").is_file());
    assert!(landing.join(".gitignore").is_file());
    let log = git(&landing, &["log", "--oneline"], None).unwrap();
    assert_eq!(log.lines().count(), 1);
    assert!(log.contains("balls: found"));

    // The store is the balls/tasks branch — a SEPARATE orphan root (no shared
    // history) with a tracked tasks/ folder.
    assert_eq!(git(&store, &["rev-parse", "--abbrev-ref", "HEAD"], None).unwrap().trim(), DEFAULT_TASKS_BRANCH);
    assert!(store.join("tasks").join(".gitkeep").is_file());
    assert_eq!(git(&store, &["log", "--oneline"], None).unwrap().lines().count(), 1);
}

#[test]
fn found_without_a_tracker_lays_no_plugin_wiring() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found(&landing, &store, None).unwrap();
    // Empty = run nothing (§12): no plugins/ subtree at all.
    assert!(!landing.join("config").join("plugins").exists());
}

#[test]
fn found_with_a_tracker_wires_and_binds_it_on_the_landing() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found(&landing, &store, Some(&fake_tracker(&tmp))).unwrap();

    // Committed relative wiring on the LANDING: import slots + every verb's post.
    for slot in ["sync/pre/50-tracker", "prime/pre/50-tracker"] {
        assert!(landing.join("config/plugins").join(slot).symlink_metadata().is_ok(), "{slot}");
    }
    for verb in ["create", "claim", "unclaim", "update", "close", "drop"] {
        let slot = format!("{verb}/post/90-tracker");
        assert!(landing.join("config/plugins").join(&slot).symlink_metadata().is_ok(), "{slot}");
    }
    // The LOCAL bin/ binding exists but is gitignored — only the portable
    // relative wiring is tracked (§2).
    assert!(landing.join("config/plugins/bin/tracker").symlink_metadata().is_ok());
    let tracked = git(&landing, &["ls-files", "config/plugins"], None).unwrap();
    assert!(tracked.contains("sync/pre/50-tracker"));
    assert!(!tracked.contains("bin/tracker"), "bin/ must not be committed");
}
