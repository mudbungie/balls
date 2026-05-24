//! Unit tests for `plugin_admin`. Standalone-mode paths use a
//! `Store::init`'d temp repo; master_url-mode paths skip the
//! state-repo materialization (that needs a reachable URL) and
//! exercise the helpers that don't reach git directly.

use super::*;
use crate::project_config::ProjectConfig;
use crate::error::{BallError, NotInitKind};
use crate::git_test_support::init_repo;
use crate::store::Store;
use std::path::Path;
use tempfile::tempdir;

fn standalone_store() -> (tempfile::TempDir, Store) {
    let td = tempdir().unwrap();
    init_repo(td.path());
    let store = Store::init(td.path(), false, None).unwrap();
    (td, store)
}

#[test]
fn validate_name_accepts_alphanumeric_and_dash_underscore() {
    assert!(validate_name("github").is_ok());
    assert!(validate_name("github-issues").is_ok());
    assert!(validate_name("ci_v2").is_ok());
}

#[test]
fn validate_name_rejects_empty_and_special_chars() {
    let err = validate_name("").unwrap_err();
    assert!(matches!(err, BallError::Other(s) if s.contains("empty")));
    let err = validate_name("foo/bar").unwrap_err();
    assert!(matches!(err, BallError::Other(s) if s.contains("ASCII")));
    let err = validate_name("../etc").unwrap_err();
    assert!(matches!(err, BallError::Other(_)));
}

#[test]
fn validate_config_file_rejects_traversal_and_absolute() {
    assert!(validate_config_file("").is_err());
    assert!(validate_config_file("/etc/passwd").is_err());
    assert!(validate_config_file("../escape.json").is_err());
    // bl-1d81: rooted at clone root — must be under .balls/plugins/
    // so the file commits alongside the project.json entry.
    assert!(validate_config_file("ok.json").is_err());
    assert!(validate_config_file("nested/ok.json").is_err());
    assert!(validate_config_file(".balls/plugins/ok.json").is_ok());
    assert!(validate_config_file(".balls/plugins/nested/ok.json").is_ok());
}

#[test]
fn enable_standalone_writes_config_and_creates_file() {
    let (_td, store) = standalone_store();
    let report = enable(&store, "github", None, true).unwrap();
    assert!(report.file_created);

    let cfg = ProjectConfig::load(&store.project_config_path()).unwrap();
    let entry = cfg.plugins.get("github").expect("entry");
    assert!(entry.enabled);
    assert!(entry.sync_on_change);
    // bl-1d81: stored path is clone-root-relative — the same base
    // `Plugin::resolve` joins against at runtime.
    assert_eq!(entry.config_file, ".balls/plugins/github.json");
    assert!(report.file_path.exists());
}

#[test]
fn enable_stores_path_runtime_can_resolve() {
    // bl-1d81 regression gate: the path `enable` records must round-
    // trip through `Plugin::resolve` to the file `enable` wrote.
    // Without this, `enable` could silently produce a config the
    // plugin subprocess can never find — the suite would stay green
    // while every `bl plugin enable` shipped a broken entry.
    use crate::plugin::Plugin;
    let (_td, store) = standalone_store();
    let report = enable(&store, "github", None, false).unwrap();
    assert!(report.file_path.exists());

    let cfg = ProjectConfig::load(&store.project_config_path()).unwrap();
    let entry = cfg.plugins.get("github").unwrap();
    let plugin = Plugin::resolve(&store, "github", entry);
    assert_eq!(plugin.config_path, report.file_path);
    assert!(plugin.config_path.exists(), "runtime must find what enable wrote");
}

#[test]
fn enable_with_explicit_config_file_resolves_to_same_file() {
    // bl-1d81: an explicit `--config-file <ROOT-RELATIVE>` must
    // round-trip the same way the default does — otherwise the bug
    // returns the moment a user passes an explicit value.
    use crate::plugin::Plugin;
    let (_td, store) = standalone_store();
    let report = enable(
        &store,
        "ci",
        Some(".balls/plugins/ci-custom.json".into()),
        false,
    )
    .unwrap();
    assert!(report.file_path.exists());

    let cfg = ProjectConfig::load(&store.project_config_path()).unwrap();
    let entry = cfg.plugins.get("ci").unwrap();
    assert_eq!(entry.config_file, ".balls/plugins/ci-custom.json");
    let plugin = Plugin::resolve(&store, "ci", entry);
    assert_eq!(plugin.config_path, report.file_path);
}

#[test]
fn enable_preserves_participant_block_on_replace() {
    use crate::config::PluginEntry;
    use crate::participant::Event;
    use crate::participant_config::{EventPolicy, ParticipantConfig, PolicyKind};
    use std::collections::BTreeMap;

    let (_td, store) = standalone_store();
    let mut cfg = ProjectConfig::load(&store.project_config_path()).unwrap();
    let mut subs = BTreeMap::new();
    subs.insert(Event::Create, EventPolicy::new(PolicyKind::BestEffort));
    cfg.plugins.insert(
        "p".into(),
        PluginEntry {
            enabled: false,
            sync_on_change: false,
            config_file: ".balls/plugins/p.json".into(),
            participant: Some(ParticipantConfig {
                subscriptions: subs,
            }),
        },
    );
    cfg.save(&store.project_config_path()).unwrap();

    enable(&store, "p", Some(".balls/plugins/p.json".into()), false).unwrap();
    let cfg = ProjectConfig::load(&store.project_config_path()).unwrap();
    let entry = cfg.plugins.get("p").unwrap();
    assert!(entry.enabled);
    assert!(
        entry.participant.is_some(),
        "participant block must survive re-enable"
    );
}

#[test]
fn enable_does_not_overwrite_existing_file() {
    let (_td, store) = standalone_store();
    let plugins_dir = effective_plugins_dir(&store);
    std::fs::create_dir_all(&plugins_dir).unwrap();
    std::fs::write(plugins_dir.join("github.json"), r#"{"k":"v"}"#).unwrap();

    let report = enable(&store, "github", None, false).unwrap();
    assert!(!report.file_created, "must reuse the existing file");
    let body = std::fs::read_to_string(&report.file_path).unwrap();
    assert!(body.contains("\"k\":\"v\""));
}

#[test]
fn enable_rejects_invalid_name() {
    let (_td, store) = standalone_store();
    assert!(enable(&store, "../bad", None, false).is_err());
}

#[test]
fn enable_rejects_invalid_config_file() {
    let (_td, store) = standalone_store();
    assert!(enable(&store, "ok", Some("../escape.json".into()), false).is_err());
}

#[test]
fn disable_removes_entry_and_keeps_file() {
    let (_td, store) = standalone_store();
    let report = enable(&store, "github", None, false).unwrap();
    let file = report.file_path.clone();

    disable(&store, "github").unwrap();
    let cfg = ProjectConfig::load(&store.project_config_path()).unwrap();
    assert!(!cfg.plugins.contains_key("github"));
    assert!(file.exists(), "config file must be kept on disable");
}

#[test]
fn disable_rejects_unknown_name() {
    let (_td, store) = standalone_store();
    let err = disable(&store, "never-enabled").unwrap_err();
    assert!(matches!(err, BallError::Other(s) if s.contains("no plugin")));
}

#[test]
fn list_returns_empty_map_on_fresh_repo() {
    let (_td, store) = standalone_store();
    assert!(load_effective(&store).unwrap().is_empty());
}

#[test]
fn list_returns_inserted_entry() {
    let (_td, store) = standalone_store();
    enable(&store, "github", None, false).unwrap();
    let plugins = load_effective(&store).unwrap();
    assert_eq!(plugins.len(), 1);
    assert!(plugins.contains_key("github"));
}

#[test]
fn load_or_default_returns_default_on_missing() {
    let td = tempdir().unwrap();
    let path = td.path().join("does-not-exist.json");
    let cfg = load_or_default(&path).unwrap();
    assert_eq!(cfg.id_length, ProjectConfig::default().id_length);
}

#[test]
fn load_or_default_propagates_other_errors() {
    let td = tempdir().unwrap();
    let path = td.path().join("broken.json");
    std::fs::write(&path, "not json").unwrap();
    let err = load_or_default(&path).unwrap_err();
    assert!(!matches!(
        err,
        BallError::NotInitialized(NotInitKind::ConfigMissing(_))
    ));
}

#[test]
fn ensure_parent_creates_missing_directory() {
    let td = tempdir().unwrap();
    let nested = td.path().join("a/b/c/file.json");
    ensure_parent(&nested).unwrap();
    assert!(nested.parent().unwrap().is_dir());
}

#[test]
fn ensure_parent_no_op_at_root() {
    // No parent (root) is a no-op, not an error.
    ensure_parent(Path::new("/")).unwrap();
}

#[test]
fn commit_change_is_a_noop_when_the_state_checkout_is_clean() {
    let (_td, store) = standalone_store();
    commit_change(&store, "nothing to commit").unwrap();
}

#[test]
fn commit_change_is_a_noop_in_a_stealth_repo() {
    let td = tempdir().unwrap();
    init_repo(td.path());
    let ext = td.path().join("ext-tasks");
    let store =
        Store::init(td.path(), false, Some(ext.to_string_lossy().into_owned())).unwrap();
    commit_change(&store, "stealth — no state checkout").unwrap();
}

#[test]
fn effective_paths_route_through_plugin_config_root() {
    let (_td, store) = standalone_store();
    let cfg_path = effective_config_path(&store);
    let plugins = effective_plugins_dir(&store);
    assert!(cfg_path.ends_with(".balls/project.json"));
    assert!(plugins.ends_with(".balls/plugins"));
    // Standalone mode roots both at the project, not a state-repo.
    assert!(cfg_path.starts_with(&store.root));
    assert!(plugins.starts_with(&store.root));
}
