use super::*;
use crate::config::{Config, PluginEntry};
use std::fs;
use tempfile::TempDir;

fn write_cfg(path: &Path, plugins: &[(&str, bool)]) {
    let mut cfg = Config::default();
    for (name, enabled) in plugins {
        cfg.plugins.insert(
            (*name).to_string(),
            PluginEntry {
                enabled: *enabled,
                sync_on_change: false,
                config_file: format!("plugins/{name}.json"),
                participant: None,
            },
        );
    }
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    cfg.save(path).unwrap();
}

#[test]
fn standalone_reads_canonical_directly() {
    let dir = TempDir::new().unwrap();
    let cfg = dir.path().join("config.json");
    let state = dir.path().join("state-wt");
    write_cfg(&cfg, &[("git", true)]);
    let effective = load_effective(&cfg, &state, false).unwrap();
    assert!(effective.plugins.contains_key("git"));
}

#[test]
fn modern_federated_through_symlink_no_layering() {
    let dir = TempDir::new().unwrap();
    let hub_cfg = dir.path().join("hub.json");
    let project_cfg = dir.path().join("config.json");
    write_cfg(&hub_cfg, &[("github", true)]);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&hub_cfg, &project_cfg).unwrap();
    let state = dir.path().to_path_buf();
    let effective = load_effective(&project_cfg, &state, true).unwrap();
    assert!(
        effective.plugins.contains_key("github"),
        "symlinked canonical reads through to the hub's plugins"
    );
}

#[test]
fn legacy_federated_layers_hub_plugins_over_project() {
    let dir = TempDir::new().unwrap();
    let project_cfg = dir.path().join("config.json");
    // Hub-side canonical, located via the state-worktree path.
    let state_wt = dir.path().join("state-wt");
    let hub_cfg = state_wt.join(".balls/config.json");
    write_cfg(&project_cfg, &[("stale-project-side", true)]);
    write_cfg(&hub_cfg, &[("authoritative-hub-side", true)]);
    let effective = load_effective(&project_cfg, &state_wt, true).unwrap();
    assert!(
        effective.plugins.contains_key("authoritative-hub-side"),
        "legacy federated must layer in the hub's plugins"
    );
    assert!(
        !effective.plugins.contains_key("stale-project-side"),
        "legacy federated must drop the project-side plugin map"
    );
}

#[test]
fn legacy_federated_with_no_hub_canonical_falls_back_to_project() {
    // Pointer says master_url is set but the hub-side canonical
    // doesn't exist yet (mid-bootstrap). The runtime should keep
    // serving the project-side plugins rather than nuking them.
    let dir = TempDir::new().unwrap();
    let project_cfg = dir.path().join("config.json");
    let state_wt = dir.path().join("state-wt");
    write_cfg(&project_cfg, &[("local-only", true)]);
    let effective = load_effective(&project_cfg, &state_wt, true).unwrap();
    assert!(effective.plugins.contains_key("local-only"));
}

#[test]
fn plugin_config_root_standalone_is_project_root() {
    let root = Path::new("/proj");
    let sw = Path::new("/proj/.balls/worktree");
    assert_eq!(plugin_config_root(root, sw, false, false), root);
}

#[test]
fn plugin_config_root_migrated_federated_is_project_root() {
    // Symlinked .balls/plugins/ → the symlink does the redirect, so
    // the project root is correct even with master_url set.
    let root = Path::new("/proj");
    let sw = Path::new("/proj/.balls/state-repo");
    assert_eq!(plugin_config_root(root, sw, true, true), root);
}

#[test]
fn plugin_config_root_legacy_federated_is_state_worktree() {
    // Unmigrated: master_url set but .balls/plugins/ still a real
    // dir — must point at the hub's state-worktree.
    let root = Path::new("/proj");
    let sw = Path::new("/proj/.balls/state-repo");
    assert_eq!(plugin_config_root(root, sw, false, true), sw);
}
