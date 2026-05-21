use super::*;
use crate::config::{Config, PluginEntry};
use std::os::unix::fs::symlink;
use tempfile::TempDir;

fn write_config(path: &Path, cfg: &Config) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    cfg.save(path).unwrap();
}

fn plug(enabled: bool) -> PluginEntry {
    PluginEntry { enabled, sync_on_change: false, config_file: "x.json".into(), participant: None }
}

#[test]
fn merge_canonical_promotes_when_hub_default() {
    let mut hub = Config::default();
    let project = Config { target_branch: Some("develop".into()), ..Config::default() };
    let mut rep = FederateReport::default();
    merge_canonical(&project, &mut hub, &mut rep);
    assert_eq!(hub.target_branch.as_deref(), Some("develop"));
}

#[test]
fn merge_canonical_hub_wins_when_set() {
    let mut hub = Config { target_branch: Some("main".into()), ..Config::default() };
    let project = Config { target_branch: Some("develop".into()), ..Config::default() };
    let mut rep = FederateReport::default();
    merge_canonical(&project, &mut hub, &mut rep);
    assert_eq!(hub.target_branch.as_deref(), Some("main"));
}

#[test]
fn merge_canonical_plugins_split_into_promoted_vs_discarded() {
    let mut hub = Config::default();
    hub.plugins.insert("github".into(), plug(true));
    let mut project = Config::default();
    project.plugins.insert("github".into(), plug(false));
    project.plugins.insert("linear".into(), plug(true));
    let mut rep = FederateReport::default();
    merge_canonical(&project, &mut hub, &mut rep);
    assert_eq!(rep.promoted_plugins, vec!["linear".to_string()]);
    assert_eq!(rep.discarded_plugins, vec!["github".to_string()]);
    assert!(hub.plugins.get("github").unwrap().enabled, "hub value survives");
}

#[test]
fn merge_canonical_clears_bootstrap_fields_on_hub() {
    let mut hub = Config {
        master_url: Some("stale".into()),
        state_remote: Some("stale".into()),
        ..Config::default()
    };
    let mut rep = FederateReport::default();
    merge_canonical(&Config::default(), &mut hub, &mut rep);
    assert_eq!(hub.master_url, None);
    assert_eq!(hub.state_remote, None);
}

#[test]
fn promote_canonical_none_stashed_is_a_noop() {
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub.json");
    let mut rep = FederateReport::default();
    promote_canonical(None, &hub, &mut rep).unwrap();
    assert!(!hub.exists(), "no stashed content ⇒ hub untouched");
}

#[test]
fn promote_canonical_writes_hub_when_absent() {
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub.json");
    let project = Config { target_branch: Some("develop".into()), ..Config::default() };
    let content = serde_json::to_string(&project).unwrap();
    let mut rep = FederateReport::default();
    promote_canonical(Some(&content), &hub, &mut rep).unwrap();
    assert_eq!(Config::load(&hub).unwrap().target_branch.as_deref(), Some("develop"));
}

#[test]
fn promote_canonical_merges_into_existing_hub() {
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub.json");
    write_config(&hub, &Config { target_branch: Some("main".into()), ..Config::default() });
    let project = Config { target_branch: Some("develop".into()), ..Config::default() };
    let content = serde_json::to_string(&project).unwrap();
    let mut rep = FederateReport::default();
    promote_canonical(Some(&content), &hub, &mut rep).unwrap();
    assert_eq!(
        Config::load(&hub).unwrap().target_branch.as_deref(),
        Some("main"),
        "hub wins when it already has the field"
    );
}

#[test]
fn stash_config_reads_and_removes_a_real_canonical() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let cfg = root.join(".balls/config.json");
    write_config(&cfg, &Config::default());
    let stashed = stash_config(root).unwrap().expect("real file ⇒ stashed");
    assert!(stashed.contains("worktree_dir"));
    assert!(!cfg.exists(), "stash_config removes the real file");
}

#[test]
fn stash_config_returns_none_for_symlink_or_absent() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".balls")).unwrap();
    assert!(stash_config(root).unwrap().is_none(), "absent ⇒ None");
    symlink("elsewhere", root.join(".balls/config.json")).unwrap();
    assert!(stash_config(root).unwrap().is_none(), "symlink ⇒ None");
}

#[test]
fn unfederate_materializes_canonical_back_and_drops_pointer() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".balls/state-repo/.balls")).unwrap();
    let hub_cfg = root.join(".balls/state-repo/.balls/config.json");
    write_config(&hub_cfg, &Config::default());
    symlink("state-repo/.balls/config.json", root.join(".balls/config.json")).unwrap();
    MasterPointer { master_url: Some("hub".into()), state_remote: None }
        .save(root)
        .unwrap();

    unfederate(root).unwrap();
    let project_cfg = root.join(".balls/config.json");
    assert!(!is_symlink(&project_cfg), "canonical restored as a real file");
    assert!(project_cfg.is_file());
    assert!(MasterPointer::load(root).unwrap().is_empty(), "pointer cleared");
}

#[test]
fn unfederate_is_idempotent_on_standalone_repo() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".balls")).unwrap();
    write_config(&root.join(".balls/config.json"), &Config::default());
    unfederate(root).unwrap(); // not federated: nothing to undo
}

#[test]
fn is_federated_requires_both_symlinks() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".balls")).unwrap();
    assert!(!is_federated(root), "standalone .balls/ is not federated");
    symlink("state-repo/.balls/config.json", root.join(".balls/config.json")).unwrap();
    assert!(!is_federated(root), "config symlink alone is not enough");
    symlink("state-repo/.balls/plugins", root.join(".balls/plugins")).unwrap();
    assert!(is_federated(root), "both symlinks ⇒ federated");
}
