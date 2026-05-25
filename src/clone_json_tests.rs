use super::*;
use crate::layered_fields::IntegrateMode;

#[test]
fn zero_keys_reads_as_no_overrides() {
    let c = CloneJson::from_json("{}").unwrap();
    assert!(!c.stealth);
    assert_eq!(c.tasks_dir, None);
    assert_eq!(c.integrate, None);
    assert_eq!(c.review, None);
    assert_eq!(c.auto_fetch_on_ready, None);
}

#[test]
fn reads_stealth_with_tasks_dir() {
    let j = r#"{"stealth": true, "tasks_dir": "/tmp/store"}"#;
    let c = CloneJson::from_json(j).unwrap();
    assert!(c.stealth);
    assert_eq!(c.tasks_dir.as_deref(), Some("/tmp/store"));
}

#[test]
fn stealth_without_tasks_dir_aborts_per_4_1() {
    let j = r#"{"stealth": true}"#;
    let e = CloneJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("tasks_dir") && s.contains("stealth"), "{s}");
}

#[test]
fn stealth_false_with_no_tasks_dir_is_fine() {
    let j = r#"{"stealth": false}"#;
    let c = CloneJson::from_json(j).unwrap();
    assert!(!c.stealth);
    assert_eq!(c.tasks_dir, None);
}

#[test]
fn tasks_dir_alone_without_stealth_does_not_trigger() {
    // tasks_dir without stealth=true is meaningless but not an
    // error — SPEC §4.1 says "ignored otherwise."
    let j = r#"{"tasks_dir": "/tmp/store"}"#;
    let c = CloneJson::from_json(j).unwrap();
    assert!(!c.stealth);
    assert_eq!(c.tasks_dir.as_deref(), Some("/tmp/store"));
}

#[test]
fn reads_layered_overrides() {
    let j = r#"{
        "integrate": {"mode": "forge-pr"},
        "review": {"gate_command": "make ci"},
        "require_remote_on_close": false,
        "auto_fetch_on_ready": false,
        "stale_threshold_seconds": 60,
        "worktree_dir": "/scratch/wt",
        "protected_main": false
    }"#;
    let c = CloneJson::from_json(j).unwrap();
    assert_eq!(c.integrate.unwrap().mode, IntegrateMode::ForgePr);
    assert_eq!(
        c.review.unwrap().gate_command.as_deref(),
        Some("make ci")
    );
    assert_eq!(c.require_remote_on_close, Some(false));
    assert_eq!(c.auto_fetch_on_ready, Some(false));
    assert_eq!(c.stale_threshold_seconds, Some(60));
    assert_eq!(c.worktree_dir.as_deref(), Some("/scratch/wt"));
    assert_eq!(c.protected_main, Some(false));
}

#[test]
fn aborts_on_tracker_scope_version_field() {
    let j = r#"{"version": 1}"#;
    let e = CloneJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("clone.json") && s.contains("version"), "{s}");
}

#[test]
fn aborts_on_tracker_scope_plugins_field() {
    let j = r#"{"plugins": {}}"#;
    let e = CloneJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("clone.json") && s.contains("plugins"), "{s}");
}

#[test]
fn aborts_on_removed_target_branch_field() {
    // The §6.7 removal applies to clone.json too — there's no
    // per-clone target_branch override either.
    let j = r#"{"target_branch": "develop"}"#;
    let e = CloneJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("target_branch"), "{s}");
}

#[test]
fn read_optional_absent_returns_none() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("nope.json");
    assert!(CloneJson::read_optional(&p).unwrap().is_none());
}

#[test]
fn read_optional_present_returns_some() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("c.json");
    std::fs::write(&p, r#"{"protected_main": false}"#).unwrap();
    let c = CloneJson::read_optional(&p).unwrap().unwrap();
    assert_eq!(c.protected_main, Some(false));
}

#[test]
fn read_optional_propagates_io_error_on_unreadable() {
    let td = tempfile::tempdir().unwrap();
    let r = CloneJson::read_optional(td.path());
    assert!(matches!(r, Err(BallError::Io(_))));
}

#[test]
fn read_optional_propagates_json_error_on_garbage() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("c.json");
    std::fs::write(&p, b"not json").unwrap();
    let e = CloneJson::read_optional(&p).unwrap_err();
    assert!(matches!(e, BallError::Json(_)));
}

#[test]
fn save_round_trips_with_nested_parent_dirs() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("a/b/c/clone.json");
    let c = CloneJson {
        stealth: true,
        tasks_dir: Some("/tmp/store".into()),
        ..Default::default()
    };
    c.save(&p).unwrap();
    let loaded = CloneJson::read_optional(&p).unwrap().unwrap();
    assert_eq!(c, loaded);
}

#[test]
fn save_omits_false_stealth_and_none_fields() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("c.json");
    CloneJson::default().save(&p).unwrap();
    let s = std::fs::read_to_string(&p).unwrap();
    assert!(!s.contains("stealth"), "{s}");
    assert!(!s.contains("tasks_dir"), "{s}");
    assert!(!s.contains("integrate"), "{s}");
}

#[test]
fn save_writes_stealth_true_when_set() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("c.json");
    CloneJson {
        stealth: true,
        tasks_dir: Some("/tmp".into()),
        ..Default::default()
    }
    .save(&p)
    .unwrap();
    let s = std::fs::read_to_string(&p).unwrap();
    assert!(s.contains("stealth"), "{s}");
    assert!(s.contains("tasks_dir"), "{s}");
}
