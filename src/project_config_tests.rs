use super::*;
use tempfile::TempDir;

fn write(dir: &TempDir, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn default_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested/project.json");
    ProjectConfig::default().save(&path).unwrap();
    let loaded = ProjectConfig::load(&path).unwrap();
    assert_eq!(loaded.version, PROJECT_SCHEMA_VERSION);
    assert_eq!(loaded.id_length, 4);
    assert_eq!(loaded.min_bl_version, None);
    assert!(loaded.plugins.is_empty());
}

#[test]
fn empty_object_fills_serde_defaults() {
    // A `{}` project.json — what `seed` writes for a fresh repo —
    // resolves every field to its built-in default.
    let dir = TempDir::new().unwrap();
    let p = write(&dir, "p.json", "{}");
    let cfg = ProjectConfig::load(&p).unwrap();
    assert_eq!(cfg.version, PROJECT_SCHEMA_VERSION);
    assert_eq!(cfg.id_length, 4);
}

#[test]
fn load_missing_returns_not_initialized() {
    let dir = TempDir::new().unwrap();
    let err = ProjectConfig::load(&dir.path().join("missing.json")).unwrap_err();
    assert!(matches!(
        err,
        BallError::NotInitialized(crate::error::NotInitKind::ConfigMissing(_))
    ));
}

#[test]
fn load_non_notfound_io_error() {
    // A directory at the path yields an IO error that is not NotFound.
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    assert!(matches!(ProjectConfig::load(&sub).unwrap_err(), BallError::Io(_)));
}

#[test]
fn load_bad_json_returns_json_error() {
    let dir = TempDir::new().unwrap();
    let p = write(&dir, "bad.json", "not json");
    assert!(matches!(ProjectConfig::load(&p).unwrap_err(), BallError::Json(_)));
}

#[test]
fn id_length_clamped_low() {
    let dir = TempDir::new().unwrap();
    let p = write(&dir, "p.json", r#"{"id_length":0}"#);
    assert_eq!(ProjectConfig::load(&p).unwrap().id_length, ID_LENGTH_MIN);
}

#[test]
fn id_length_clamped_high() {
    let dir = TempDir::new().unwrap();
    let p = write(&dir, "p.json", r#"{"id_length":99}"#);
    assert_eq!(ProjectConfig::load(&p).unwrap().id_length, ID_LENGTH_MAX);
}

#[test]
fn load_rejects_future_schema_version() {
    let dir = TempDir::new().unwrap();
    let p = write(
        &dir,
        "p.json",
        &format!(r#"{{"version":{}}}"#, PROJECT_SCHEMA_VERSION + 1),
    );
    let err = ProjectConfig::load(&p).unwrap_err();
    assert!(
        matches!(err, BallError::Other(ref s) if s.contains("schema version") && s.contains("upgrade bl")),
        "got {err:?}"
    );
}

#[test]
fn min_bl_version_skipped_when_none_and_round_trips_when_set() {
    let s = serde_json::to_string(&ProjectConfig::default()).unwrap();
    assert!(!s.contains("min_bl_version"), "default must not serialize it: {s}");
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("p.json");
    // 9.9.9 is above this build — load exercises the advisory warn
    // path without the advisory ever becoming an error.
    let cfg = ProjectConfig { min_bl_version: Some("9.9.9".into()), ..ProjectConfig::default() };
    cfg.save(&path).unwrap();
    let loaded = ProjectConfig::load(&path).unwrap();
    assert_eq!(loaded.min_bl_version.as_deref(), Some("9.9.9"));
}

#[test]
fn plugin_entry_serde() {
    let mut cfg = ProjectConfig::default();
    cfg.plugins.insert(
        "jira".into(),
        PluginEntry {
            enabled: true,
            sync_on_change: true,
            config_file: ".balls/plugins/jira.json".into(),
            participant: None,
        },
    );
    let s = serde_json::to_string(&cfg).unwrap();
    assert!(s.contains("jira"));
    let back: ProjectConfig = serde_json::from_str(&s).unwrap();
    assert_eq!(back.plugins.len(), 1);
}

// SPEC §6.2 / §17.20: `drop` is observe-only — `required`/`gating`
// on it must fail validation; `best-effort` is the only legal policy.
#[test]
fn drop_with_required_policy_is_rejected() {
    let dir = TempDir::new().unwrap();
    let p = write(
        &dir,
        "p.json",
        r#"{"plugins":{"jira":{"enabled":true,"sync_on_change":false,
            "config_file":".balls/plugins/jira.json",
            "participant":{"subscriptions":{"drop":{"policy":"required"}}}}}}"#,
    );
    let err = ProjectConfig::load(&p).unwrap_err();
    assert!(matches!(err, BallError::Other(ref s) if s.contains("observe-only") && s.contains("jira")));
}

#[test]
fn drop_with_best_effort_policy_is_accepted() {
    let dir = TempDir::new().unwrap();
    let p = write(
        &dir,
        "p.json",
        r#"{"plugins":{"jira":{"enabled":true,"sync_on_change":false,
            "config_file":".balls/plugins/jira.json",
            "participant":{"subscriptions":{"drop":{"policy":"best-effort"}}}}}}"#,
    );
    assert!(ProjectConfig::load(&p).unwrap().plugins.contains_key("jira"));
}

#[test]
fn validate_skips_a_plugin_without_a_drop_subscription() {
    // A participant block carrying only non-`drop` events takes the
    // `else { continue }` arm and validates clean.
    let dir = TempDir::new().unwrap();
    let p = write(
        &dir,
        "p.json",
        r#"{"plugins":{"jira":{"enabled":true,"sync_on_change":false,
            "config_file":".balls/plugins/jira.json",
            "participant":{"subscriptions":{"close":{"policy":"required"}}}}}}"#,
    );
    assert!(ProjectConfig::load(&p).unwrap().plugins.contains_key("jira"));
}

#[test]
fn validate_skips_a_plugin_without_a_participant_block() {
    // No participant block at all: the drop-policy gate has nothing to
    // inspect and the plugin validates clean.
    let dir = TempDir::new().unwrap();
    let p = write(
        &dir,
        "p.json",
        r#"{"plugins":{"gh":{"enabled":true,"sync_on_change":false,
            "config_file":".balls/plugins/gh.json"}}}"#,
    );
    assert!(ProjectConfig::load(&p).unwrap().plugins.contains_key("gh"));
}

#[test]
fn from_config_file_reads_a_pre_split_config() {
    // A config.json still carrying project-owned fields — the
    // pre-split shape — is migrated through this lenient reader.
    let dir = TempDir::new().unwrap();
    let p = write(
        &dir,
        "config.json",
        r#"{"version":1,"id_length":7,"stale_threshold_seconds":60,
            "worktree_dir":".balls-worktrees",
            "plugins":{"gh":{"enabled":true,"sync_on_change":false,
              "config_file":".balls/plugins/gh.json"}}}"#,
    );
    let pc = ProjectConfig::from_config_file(&p);
    assert_eq!(pc.id_length, 7);
    assert!(pc.plugins.contains_key("gh"));
}

#[test]
fn from_config_file_missing_yields_defaults() {
    let dir = TempDir::new().unwrap();
    let pc = ProjectConfig::from_config_file(&dir.path().join("nope.json"));
    assert_eq!(pc.id_length, 4);
    assert!(pc.plugins.is_empty());
}

#[test]
fn from_config_file_unparseable_yields_defaults() {
    let dir = TempDir::new().unwrap();
    let p = write(&dir, "config.json", "{not json");
    assert_eq!(ProjectConfig::from_config_file(&p).id_length, 4);
}

#[test]
fn from_config_file_clamps_id_length() {
    let dir = TempDir::new().unwrap();
    let p = write(&dir, "config.json", r#"{"id_length":0}"#);
    assert_eq!(ProjectConfig::from_config_file(&p).id_length, ID_LENGTH_MIN);
}

#[test]
fn resolve_prefers_project_json_when_present() {
    // project.json wins outright — config.json's stale value is shadowed.
    let dir = TempDir::new().unwrap();
    let project = write(&dir, "project.json", r#"{"id_length":9}"#);
    let config = write(&dir, "config.json", r#"{"id_length":5}"#);
    assert_eq!(ProjectConfig::resolve(&project, &config).unwrap().id_length, 9);
}

#[test]
fn resolve_falls_back_to_config_json_without_project_json() {
    // No project.json — a stealth or pre-split repo reads project-owned
    // fields from config.json.
    let dir = TempDir::new().unwrap();
    let config = write(&dir, "config.json", r#"{"id_length":6}"#);
    let res = ProjectConfig::resolve(&dir.path().join("project.json"), &config).unwrap();
    assert_eq!(res.id_length, 6);
}

#[test]
fn resolve_fallback_validates_the_config_json_view() {
    // The config.json fallback is still validated: a bad drop policy
    // there fails resolve rather than silently degrading.
    let dir = TempDir::new().unwrap();
    let config = write(
        &dir,
        "config.json",
        r#"{"plugins":{"jira":{"enabled":true,"sync_on_change":false,
            "config_file":".balls/plugins/jira.json",
            "participant":{"subscriptions":{"drop":{"policy":"gating"}}}}}}"#,
    );
    let err = ProjectConfig::resolve(&dir.path().join("project.json"), &config).unwrap_err();
    assert!(matches!(err, BallError::Other(ref s) if s.contains("observe-only")));
}

