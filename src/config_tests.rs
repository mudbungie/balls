use super::*;
use tempfile::TempDir;

#[test]
fn default_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested/config.json");
    let cfg = Config::default();
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.id_length, 4);
    assert!(loaded.auto_fetch_on_ready);
    assert!(!loaded.protected_main);
    assert!(loaded.plugins.is_empty());
}

#[test]
fn load_missing_returns_not_initialized() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.json");
    let err = Config::load(&path).unwrap_err();
    assert!(matches!(
        err,
        BallError::NotInitialized(crate::error::NotInitKind::ConfigMissing(_))
    ));
    assert!(err.to_string().contains("not initialized"));
}

#[test]
fn load_bad_json_returns_json_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bad.json");
    std::fs::write(&path, "not json").unwrap();
    let err = Config::load(&path).unwrap_err();
    assert!(matches!(err, BallError::Json(_)));
}

#[test]
fn default_true_fills_in_missing_field() {
    // Omit auto_fetch_on_ready — serde default must be true
    let s = r#"{
        "version": 1,
        "id_length": 4,
        "stale_threshold_seconds": 60,
        "worktree_dir": ".balls-worktrees"
    }"#;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.json");
    std::fs::write(&path, s).unwrap();
    let cfg = Config::load(&path).unwrap();
    assert!(cfg.auto_fetch_on_ready);
}

#[test]
fn load_non_notfound_io_error() {
    // A directory at the config path yields an IO error that's not NotFound.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sub");
    std::fs::create_dir_all(&path).unwrap();
    let err = Config::load(&path).unwrap_err();
    assert!(matches!(err, BallError::Io(_)));
}

fn write_cfg(dir: &TempDir, body: &str) -> std::path::PathBuf {
    let path = dir.path().join("c.json");
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn id_length_clamped_low() {
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"version":1,"id_length":0,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}"#,
    );
    let cfg = Config::load(&p).unwrap();
    assert_eq!(cfg.id_length, ID_LENGTH_MIN);
}

#[test]
fn id_length_clamped_high() {
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"version":1,"id_length":99,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}"#,
    );
    let cfg = Config::load(&p).unwrap();
    assert_eq!(cfg.id_length, ID_LENGTH_MAX);
}

#[test]
fn worktree_dir_rejects_absolute_path() {
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":"/tmp/evil"}"#,
    );
    let err = Config::load(&p).unwrap_err();
    assert!(matches!(err, BallError::Other(ref s) if s.contains("worktree_dir")));
}

#[test]
fn worktree_dir_rejects_parent_segment() {
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":"../escape"}"#,
    );
    let err = Config::load(&p).unwrap_err();
    assert!(matches!(err, BallError::Other(ref s) if s.contains("worktree_dir")));
}

#[test]
fn load_rejects_future_schema_version() {
    let dir = TempDir::new().unwrap();
    let future = CONFIG_SCHEMA_VERSION + 1;
    let p = write_cfg(
        &dir,
        &format!(
            r#"{{"version":{future},"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}}"#
        ),
    );
    let err = Config::load(&p).unwrap_err();
    assert!(
        matches!(err, BallError::Other(ref s) if s.contains("schema version") && s.contains("upgrade bl")),
        "expected schema-version error, got: {err:?}",
    );
}

#[test]
fn load_accepts_current_schema_version() {
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        &format!(
            r#"{{"version":{CONFIG_SCHEMA_VERSION},"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}}"#
        ),
    );
    let cfg = Config::load(&p).unwrap();
    assert_eq!(cfg.version, CONFIG_SCHEMA_VERSION);
}

#[test]
fn plugin_entry_serde() {
    let mut cfg = Config::default();
    cfg.plugins.insert(
        "jira".to_string(),
        PluginEntry {
            enabled: true,
            sync_on_change: true,
            config_file: ".balls/plugins/jira.json".into(),
            participant: None,
        },
    );
    let s = serde_json::to_string(&cfg).unwrap();
    assert!(s.contains("jira"));
    let back: Config = serde_json::from_str(&s).unwrap();
    assert_eq!(back.plugins.len(), 1);
}

#[test]
fn state_remote_default_is_origin() {
    // Unset field resolves to the historical hardcoded remote, so a
    // single-repo setup is byte-identical to before this field.
    let cfg = Config::default();
    assert_eq!(cfg.state_remote, None);
    assert_eq!(cfg.state_remote(), "origin");
    assert_eq!(cfg.state_remote(), DEFAULT_STATE_REMOTE);
}

#[test]
fn state_remote_none_is_omitted_from_serialization() {
    // skip_serializing_if keeps an unmodified config byte-identical:
    // the key must not appear when unset.
    let s = serde_json::to_string(&Config::default()).unwrap();
    assert!(
        !s.contains("state_remote"),
        "default config must not serialize state_remote: {s}"
    );
}

#[test]
fn state_remote_explicit_value_resolves_and_round_trips() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.json");
    let cfg = Config {
        state_remote: Some("taskhub".to_string()),
        ..Config::default()
    };
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.state_remote.as_deref(), Some("taskhub"));
    assert_eq!(loaded.state_remote(), "taskhub");
}

#[test]
fn target_branch_none_is_omitted_from_serialization() {
    // skip_serializing_if keeps an unmodified config byte-identical:
    // the key must not appear when unset, mirroring state_remote.
    let cfg = Config::default();
    assert_eq!(cfg.target_branch, None);
    let s = serde_json::to_string(&cfg).unwrap();
    assert!(
        !s.contains("target_branch"),
        "default config must not serialize target_branch: {s}"
    );
}

#[test]
fn target_branch_explicit_value_short_circuits_git_and_round_trips() {
    // A configured target_branch wins outright: `integration_branch`
    // returns it without consulting git, so a path that isn't even a
    // repo still resolves. This is the single-seam guarantee.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("c.json");
    let cfg = Config {
        target_branch: Some("develop".to_string()),
        ..Config::default()
    };
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.target_branch.as_deref(), Some("develop"));
    assert_eq!(
        loaded
            .integration_branch(std::path::Path::new("/no/such/repo"))
            .unwrap(),
        "develop"
    );
}

// SPEC §6.2 / §17.20: `drop` is observe-only. `required`/`gating` on
// it must fail config validation; `best-effort` is the only legal
// policy.
#[test]
fn drop_with_required_policy_is_rejected() {
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,
            "worktree_dir":".balls-worktrees",
            "plugins":{"jira":{"enabled":true,"sync_on_change":false,
              "config_file":".balls/plugins/jira.json",
              "participant":{"subscriptions":{"drop":{"policy":"required"}}}}}}"#,
    );
    let err = Config::load(&p).unwrap_err();
    assert!(
        matches!(err, BallError::Other(ref s)
            if s.contains("observe-only") && s.contains("jira")),
        "got {err}"
    );
}

#[test]
fn drop_with_best_effort_policy_is_accepted() {
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,
            "worktree_dir":".balls-worktrees",
            "plugins":{"jira":{"enabled":true,"sync_on_change":false,
              "config_file":".balls/plugins/jira.json",
              "participant":{"subscriptions":{"drop":{"policy":"best-effort"}}}}}}"#,
    );
    let cfg = Config::load(&p).unwrap();
    assert!(cfg.plugins.contains_key("jira"));
}

#[test]
fn legacy_config_without_state_remote_loads_and_defaults() {
    // A config written before this field existed has no state_remote
    // key; serde default keeps it loadable and it resolves to origin.
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}"#,
    );
    let cfg = Config::load(&p).unwrap();
    assert_eq!(cfg.state_remote, None);
    assert_eq!(cfg.state_remote(), "origin");
}

#[test]
fn delivery_mode_defaults_and_round_trips() {
    // Unset delivery never serializes and resolves to local-squash.
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("d.json");
    let mut cfg = Config::default();
    cfg.save(&p).unwrap();
    assert!(!std::fs::read_to_string(&p).unwrap().contains("delivery"));
    assert_eq!(cfg.delivery_mode(), DeliveryMode::LocalSquash);
    cfg.delivery = Some(Delivery { mode: DeliveryMode::Deferred });
    cfg.save(&p).unwrap();
    assert!(std::fs::read_to_string(&p).unwrap().contains(r#""mode": "deferred""#));
    assert_eq!(Config::load(&p).unwrap().delivery_mode(), DeliveryMode::Deferred);
}
