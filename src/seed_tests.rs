//! Tests for the §1/§12 seed — embedded bootstrap, the XDG override, the
//! bind-present + prune-absent, and the established-landing rebind. Each builds
//! throwaway XDG / landing / exe dirs in a tempdir; no real binary is run (a
//! bind is just a symlink — protocol validation is `install`'s job, §6).

use super::*;
use crate::hooks::Hooks;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// An `Xdg` whose config-home is `root/cfg` (so `default-config/` lands there).
fn xdg_at(root: &Path) -> Xdg {
    Xdg::with(root, Some(&root.join("cfg").to_string_lossy()), Some(&root.join("state").to_string_lossy()))
}

/// A dir holding fake sibling binaries `names`, standing in for the dir `bl`
/// lives in.
fn exe_dir_with(root: &Path, names: &[&str]) -> std::path::PathBuf {
    let dir = root.join("bin");
    fs::create_dir_all(&dir).unwrap();
    for name in names {
        fs::write(dir.join(name), "#!/bin/sh\n").unwrap();
    }
    dir
}

fn bin_link(landing: &Path, name: &str) -> std::path::PathBuf {
    landing.join("config/plugins/bin").join(name)
}

#[test]
fn seed_writes_the_embedded_default_then_copies_and_binds_present_plugins() {
    let tmp = TempDir::new().unwrap();
    let xdg = xdg_at(tmp.path());
    let landing = tmp.path().join("landing");
    let exe = exe_dir_with(tmp.path(), &["bl-tracker", "bl-delivery"]);

    seed_landing(&xdg, &landing, Some(&exe)).unwrap();

    // The embedded default was materialized to the XDG override slot (absent → write).
    assert!(xdg.default_config().join("balls.toml").is_file());
    assert!(xdg.default_config().join("plugins.toml").is_file());
    // The landing's config/ was seeded from it.
    assert!(landing.join("config/balls.toml").is_file());
    // Both shipped plugins resolved → kept in the schedule and bound locally.
    let hooks = Hooks::load(&landing).unwrap();
    assert_eq!(hooks.names("close", "post"), ["bl-delivery", "bl-tracker"]);
    assert!(bin_link(&landing, "bl-tracker").symlink_metadata().is_ok());
    assert!(bin_link(&landing, "bl-delivery").symlink_metadata().is_ok());
    // The default wires no pre-seal staging (bl-0af4 — nothing is stored) but
    // does wire the `show` read-op under its bare key (§6 read dispatch).
    assert!(hooks.names("claim", "pre").is_empty());
    assert!(hooks.names("unclaim", "pre").is_empty());
    assert_eq!(hooks.resolve_read(&crate::registry::Registry::at(&landing), "show").len(), 1);
}

#[test]
fn seed_prunes_a_plugin_whose_binary_is_absent_here() {
    let tmp = TempDir::new().unwrap();
    let xdg = xdg_at(tmp.path());
    let landing = tmp.path().join("landing");
    // Only bl-tracker is installed beside bl — bl-delivery must prune.
    let exe = exe_dir_with(tmp.path(), &["bl-tracker"]);

    seed_landing(&xdg, &landing, Some(&exe)).unwrap();

    let hooks = Hooks::load(&landing).unwrap();
    assert_eq!(hooks.names("close", "post"), ["bl-tracker"]); // bl-delivery dropped
    assert!(hooks.names("close", "pre").is_empty()); // was [bl-delivery] → emptied
    assert!(bin_link(&landing, "bl-tracker").symlink_metadata().is_ok());
    assert!(bin_link(&landing, "bl-delivery").symlink_metadata().is_err());
}

#[test]
fn seed_with_no_exe_dir_prunes_every_plugin() {
    let tmp = TempDir::new().unwrap();
    let xdg = xdg_at(tmp.path());
    let landing = tmp.path().join("landing");

    seed_landing(&xdg, &landing, None).unwrap();

    // A tracker-less box: every default entry prunes, the chain runs empty.
    let hooks = Hooks::load(&landing).unwrap();
    assert!(hooks.names("prime", "pre").is_empty());
    assert!(!landing.join("config/plugins/bin").exists());
    // balls.toml still seeded — config defaults stand.
    assert!(landing.join("config/balls.toml").is_file());
}

#[test]
fn an_xdg_override_wins_over_the_embedded_default() {
    let tmp = TempDir::new().unwrap();
    let xdg = xdg_at(tmp.path());
    // Pre-populate the override folder with a custom schedule (only a tracker).
    let dc = xdg.default_config();
    fs::create_dir_all(&dc).unwrap();
    fs::write(dc.join("balls.toml"), "tasks_branch = \"team/tasks\"\n").unwrap();
    fs::write(dc.join("plugins.toml"), "[hooks]\n\"close.post\" = [\"tracker\"]\n").unwrap();

    let landing = tmp.path().join("landing");
    let exe = exe_dir_with(tmp.path(), &["tracker", "bl-delivery"]);
    seed_landing(&xdg, &landing, Some(&exe)).unwrap();

    // The override's config travelled, not the embedded one.
    assert!(fs::read_to_string(landing.join("config/balls.toml")).unwrap().contains("team/tasks"));
    let hooks = Hooks::load(&landing).unwrap();
    assert_eq!(hooks.names("close", "post"), ["tracker"]); // override schedule
    assert!(hooks.names("prime", "pre").is_empty()); // override wired no prime hook
}

#[test]
fn an_override_missing_a_file_seeds_only_what_it_has() {
    let tmp = TempDir::new().unwrap();
    let xdg = xdg_at(tmp.path());
    // An override with a plugins.toml but no balls.toml.
    let dc = xdg.default_config();
    fs::create_dir_all(&dc).unwrap();
    fs::write(dc.join("plugins.toml"), "[hooks]\n\"create.post\" = [\"tracker\"]\n").unwrap();

    let landing = tmp.path().join("landing");
    let exe = exe_dir_with(tmp.path(), &["tracker"]);
    seed_landing(&xdg, &landing, Some(&exe)).unwrap();

    // No balls.toml in the override → none in the landing (the field defaults stand).
    assert!(!landing.join("config/balls.toml").exists());
    assert_eq!(Hooks::load(&landing).unwrap().names("create", "post"), ["tracker"]);
}

#[test]
fn rebind_reestablishes_present_bindings_without_touching_the_schedule() {
    let tmp = TempDir::new().unwrap();
    let landing = tmp.path().join("landing");
    let config = landing.join("config");
    fs::create_dir_all(&config).unwrap();
    let schedule = "[hooks]\n\"close.post\" = [\"bl-delivery\", \"tracker\"]\n";
    fs::write(config.join("plugins.toml"), schedule).unwrap();
    // A new machine has only the tracker installed.
    let exe = exe_dir_with(tmp.path(), &["tracker"]);

    rebind(&landing, Some(&exe)).unwrap();

    // The present binary is bound; the absent one stays dangling (no prune).
    assert!(bin_link(&landing, "tracker").symlink_metadata().is_ok());
    assert!(bin_link(&landing, "bl-delivery").symlink_metadata().is_err());
    // The committed schedule is untouched — capabilities change only by install.
    assert_eq!(fs::read_to_string(config.join("plugins.toml")).unwrap(), schedule);
}
