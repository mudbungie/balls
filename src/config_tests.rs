use super::*;
use tempfile::TempDir;

#[test]
fn default_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested/config.json");
    let cfg = Config::default();
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.stale_threshold_seconds, 60);
    assert_eq!(loaded.worktree_dir, ".balls-worktrees");
    assert!(loaded.auto_fetch_on_ready);
    assert!(!loaded.protected_main);
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
    // Omit auto_fetch_on_ready — serde default must be true. The
    // project-owned `version`/`id_length` keys are inert here: `Config`
    // ignores them under the SPEC §7 split.
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
fn worktree_dir_rejects_absolute_path() {
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"stale_threshold_seconds":60,"worktree_dir":"/tmp/evil"}"#,
    );
    let err = Config::load(&p).unwrap_err();
    assert!(matches!(err, BallError::Other(ref s) if s.contains("worktree_dir")));
}

#[test]
fn worktree_dir_rejects_parent_segment() {
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"stale_threshold_seconds":60,"worktree_dir":"../escape"}"#,
    );
    let err = Config::load(&p).unwrap_err();
    assert!(matches!(err, BallError::Other(ref s) if s.contains("worktree_dir")));
}

#[test]
fn load_accepts_non_default_state_branch() {
    // bl-3f59 wired state_branch end to end (push/fetch/merge refspecs
    // and bl remaster --branch); a non-default branch is now a
    // first-class supported value, not a gate-rejected one.
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"stale_threshold_seconds":60,
            "worktree_dir":".balls-worktrees","state_branch":"project-x"}"#,
    );
    let cfg = Config::load(&p).unwrap();
    assert_eq!(cfg.state_branch.as_deref(), Some("project-x"));
}

#[test]
fn legacy_config_without_state_remote_loads_and_defaults() {
    // A config written before this field existed has no state_remote
    // key; serde default keeps it loadable and `None`.
    let dir = TempDir::new().unwrap();
    let p = write_cfg(
        &dir,
        r#"{"stale_threshold_seconds":60,"worktree_dir":".balls-worktrees"}"#,
    );
    let cfg = Config::load(&p).unwrap();
    assert_eq!(cfg.state_remote, None);
}
