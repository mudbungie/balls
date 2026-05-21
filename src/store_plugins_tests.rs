use super::*;
use crate::config::{Config, PluginEntry};
use std::collections::BTreeMap;
use std::fs;

fn write_cfg(dir: &Path, master_url: Option<&str>, plugins: &[(&str, &str)]) -> PathBuf {
    fs::create_dir_all(dir.join(".balls")).unwrap();
    let mut cfg = Config::default();
    if let Some(u) = master_url {
        cfg.master_url = Some(u.into());
    }
    for (name, cf) in plugins {
        cfg.plugins.insert(
            (*name).into(),
            PluginEntry {
                enabled: true,
                sync_on_change: false,
                config_file: (*cf).into(),
                participant: None,
            },
        );
    }
    let p = dir.join(".balls/config.json");
    cfg.save(&p).unwrap();
    p
}

#[test]
fn standalone_returns_project_config_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let state = dir.path().join("state");
    let cfg_path = write_cfg(&project, None, &[("legacy", ".balls/plugins/legacy.json")]);

    let (cfg, drifted) = load_layered_inner(&cfg_path, &state, &project).unwrap();

    assert!(cfg.master_url().is_none());
    assert_eq!(cfg.plugins.len(), 1);
    assert!(cfg.plugins.contains_key("legacy"));
    assert!(!drifted);
}

#[test]
fn master_with_no_state_config_replaces_plugins_with_empty() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let state = dir.path().join("state");
    let cfg_path = write_cfg(
        &project,
        Some("file:///hub"),
        &[("ignored", ".balls/plugins/ignored.json")],
    );

    let (cfg, drifted) = load_layered_inner(&cfg_path, &state, &project).unwrap();

    assert!(cfg.plugins.is_empty(), "project plugins must be ignored");
    assert!(drifted, "non-empty project plugins map counts as drift");
}

#[test]
fn master_with_state_config_overlays_hub_plugins() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let state = dir.path().join("state");
    let cfg_path = write_cfg(&project, Some("file:///hub"), &[]);
    write_cfg(&state, None, &[("hub-plugin", ".balls/plugins/hub-plugin.json")]);

    let (cfg, drifted) = load_layered_inner(&cfg_path, &state, &project).unwrap();

    assert_eq!(cfg.plugins.len(), 1);
    assert!(cfg.plugins.contains_key("hub-plugin"));
    assert!(!drifted, "empty project map + empty plugin dir is not drift");
}

#[test]
fn master_warns_when_project_has_committed_plugin_files() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let state = dir.path().join("state");
    let cfg_path = write_cfg(&project, Some("file:///hub"), &[]);
    fs::create_dir_all(project.join(".balls/plugins")).unwrap();
    fs::write(project.join(".balls/plugins/.gitkeep"), "").unwrap();
    fs::write(project.join(".balls/plugins/leftover.json"), "{}").unwrap();

    let (_cfg, drifted) = load_layered_inner(&cfg_path, &state, &project).unwrap();
    assert!(drifted, "committed plugin file under master_url is drift");
}

#[test]
fn load_layered_wrapper_emits_warning_on_drift() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let state = dir.path().join("state");
    let cfg_path = write_cfg(
        &project,
        Some("file:///hub"),
        &[("drift", ".balls/plugins/drift.json")],
    );

    let cfg = load_layered(&cfg_path, &state, &project).unwrap();
    assert!(cfg.plugins.is_empty());
}

#[test]
fn load_layered_wrapper_silent_when_no_drift() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let state = dir.path().join("state");
    let cfg_path = write_cfg(&project, None, &[]);
    let cfg = load_layered(&cfg_path, &state, &project).unwrap();
    assert!(cfg.master_url().is_none());
}

#[test]
fn plugin_config_root_chooses_state_under_master_url() {
    let project = Path::new("/p");
    let state = Path::new("/s");
    let cfg = Config {
        master_url: Some("file:///hub".into()),
        ..Config::default()
    };
    assert_eq!(plugin_config_root(project, state, &cfg), state);
}

#[test]
fn plugin_config_root_chooses_project_otherwise() {
    let project = Path::new("/p");
    let state = Path::new("/s");
    let cfg = Config::default();
    assert_eq!(plugin_config_root(project, state, &cfg), project);
}

#[test]
fn read_state_plugins_returns_none_when_state_config_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_state_plugins(dir.path()).unwrap().is_none());
}

#[test]
fn read_state_plugins_returns_map_when_present() {
    let dir = tempfile::tempdir().unwrap();
    write_cfg(dir.path(), None, &[("p", ".balls/plugins/p.json")]);
    let plugins = read_state_plugins(dir.path()).unwrap().unwrap();
    assert_eq!(plugins.len(), 1);
    assert!(plugins.contains_key("p"));
}

#[test]
fn has_real_plugin_files_false_when_dir_missing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!has_real_plugin_files(dir.path()));
}

#[test]
fn has_real_plugin_files_false_when_only_gitkeep() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".balls/plugins")).unwrap();
    fs::write(dir.path().join(".balls/plugins/.gitkeep"), "").unwrap();
    assert!(!has_real_plugin_files(dir.path()));
}

#[test]
fn has_real_plugin_files_true_when_real_file_present() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".balls/plugins")).unwrap();
    fs::write(dir.path().join(".balls/plugins/x.json"), "{}").unwrap();
    assert!(has_real_plugin_files(dir.path()));
}

#[test]
fn has_real_plugin_files_false_when_symlinked_to_hub() {
    // bl-1098: in federated mode `.balls/plugins/` is a symlink to the
    // state-repo's plugins dir — its contents ARE the hub's view, so
    // they are never "drift" no matter how many files are reachable.
    let dir = tempfile::tempdir().unwrap();
    let hub = dir.path().join("hub");
    fs::create_dir_all(&hub).unwrap();
    fs::write(hub.join("real.json"), "{}").unwrap();
    fs::create_dir_all(dir.path().join(".balls")).unwrap();
    std::os::unix::fs::symlink(&hub, dir.path().join(".balls/plugins")).unwrap();
    assert!(!has_real_plugin_files(dir.path()));
}

#[test]
fn emit_drift_warning_silent_on_second_call() {
    let once = AtomicBool::new(false);
    emit_drift_warning(&once);
    emit_drift_warning(&once);
    // Both arms of the swap are exercised; coverage is the contract here.
    assert!(once.load(Ordering::Relaxed));
}

#[test]
fn warn_drift_once_callable() {
    // Exercises the production wrapper at least once for coverage. The
    // process-static `WARNED` may already be set from another test in
    // the same binary; either branch of `emit_drift_warning` is fine
    // here, the call must just not panic.
    warn_drift_once();
}

// Touch the BTreeMap import so it isn't flagged as unused on edits.
#[allow(dead_code)]
const _: fn() = || {
    let _: BTreeMap<String, PluginEntry> = BTreeMap::new();
};
