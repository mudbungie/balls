//! Tests for §12 bootstrap-on-miss on throwaway repos — `found` makes BOTH
//! branches of the two-branch substrate (the `balls/config` landing + the
//! `balls/tasks` store) and seeds the landing's config from the app
//! default-config (§1/§12), with and without the shipped plugin binaries.

use super::*;
use crate::git::run as git;
use crate::hooks::Hooks;
use crate::layout::Xdg;
use tempfile::TempDir;

/// The two checkout paths under a fresh tempdir (neither need pre-exist — `found`
/// creates the landing repo and adds the store as a linked worktree).
fn paths(tmp: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    (tmp.path().join("config"), tmp.path().join("tasks"))
}

/// An `Xdg` rooted under `tmp` so the embedded default-config materializes in the
/// tempdir, never the real `$XDG_CONFIG_HOME`.
fn xdg(tmp: &TempDir) -> Xdg {
    Xdg::with(tmp.path(), Some(&tmp.path().join("cfg").to_string_lossy()), Some(&tmp.path().join("st").to_string_lossy()))
}

/// A dir holding fake sibling binaries, standing in for the dir `bl` lives in.
fn exe_dir(tmp: &TempDir, names: &[&str]) -> std::path::PathBuf {
    let dir = tmp.path().join("bin");
    fs::create_dir_all(&dir).unwrap();
    for name in names {
        fs::write(dir.join(name), "#!/bin/sh\n").unwrap();
    }
    dir
}

#[test]
fn found_makes_both_branches_with_seeded_config_and_store() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found(&landing, &store, &xdg(&tmp), None).unwrap();

    // The landing is the balls/config branch with a seeded config/.
    assert_eq!(git(&landing, &["rev-parse", "--abbrev-ref", "HEAD"], None).unwrap().trim(), LANDING_BRANCH);
    assert!(landing.join("config").join("balls.toml").is_file());
    assert!(landing.join("config").join("plugins.toml").is_file());
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
fn found_without_any_plugin_binary_seeds_an_empty_schedule() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    found(&landing, &store, &xdg(&tmp), None).unwrap();
    // The schedule file exists but every default entry pruned (no binaries here).
    let hooks = Hooks::load(&landing).unwrap();
    assert!(hooks.names("prime", "pre").is_empty());
    assert!(!landing.join("config/plugins/bin").exists());
}

#[test]
fn found_with_the_shipped_binaries_keeps_and_binds_them_on_the_landing() {
    let tmp = TempDir::new().unwrap();
    let (landing, store) = paths(&tmp);
    let exe = exe_dir(&tmp, &["tracker", "bl-delivery"]);
    found(&landing, &store, &xdg(&tmp), Some(&exe)).unwrap();

    // The committed schedule keeps both shipped plugins (§6).
    let hooks = Hooks::load(&landing).unwrap();
    assert_eq!(hooks.names("create", "post"), ["tracker"]);
    assert_eq!(hooks.names("close", "post"), ["bl-delivery", "tracker"]);
    // The LOCAL bin/ bindings exist but are gitignored — only the committed text
    // (balls.toml + plugins.toml) is tracked (§2).
    assert!(landing.join("config/plugins/bin/tracker").symlink_metadata().is_ok());
    assert!(landing.join("config/plugins/bin/bl-delivery").symlink_metadata().is_ok());
    let tracked = git(&landing, &["ls-files", "config"], None).unwrap();
    assert!(tracked.contains("config/plugins.toml"));
    assert!(!tracked.contains("bin/tracker"), "bin/ must not be committed");
}
