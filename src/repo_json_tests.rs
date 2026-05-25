use super::*;
use crate::layered_fields::IntegrateMode;

#[test]
fn zero_keys_reads_as_all_defaults() {
    let r = RepoJson::from_json("{}").unwrap();
    assert_eq!(r.integrate, None);
    assert_eq!(r.review, None);
    assert_eq!(r.require_remote_on_claim, None);
    assert_eq!(r.require_remote_on_review, None);
    assert_eq!(r.require_remote_on_close, None);
    assert!(r.auto_fetch_on_ready);
    assert_eq!(r.stale_threshold_seconds, DEFAULT_STALE_THRESHOLD);
    assert_eq!(r.worktree_dir, None);
    assert!(r.protected_main);
}

#[test]
fn reads_explicit_layered_fields() {
    let j = r#"{
        "integrate": {"mode": "forge-pr"},
        "review": {"gate_command": "make check"},
        "require_remote_on_claim": false
    }"#;
    let r = RepoJson::from_json(j).unwrap();
    assert_eq!(r.integrate.unwrap().mode, IntegrateMode::ForgePr);
    assert_eq!(
        r.review.unwrap().gate_command.as_deref(),
        Some("make check")
    );
    assert_eq!(r.require_remote_on_claim, Some(false));
    // Untouched layered fields stay None for fall-through.
    assert_eq!(r.require_remote_on_review, None);
}

#[test]
fn reads_repo_only_fields() {
    let j = r#"{
        "auto_fetch_on_ready": false,
        "stale_threshold_seconds": 42,
        "worktree_dir": "/tmp/wt",
        "protected_main": false
    }"#;
    let r = RepoJson::from_json(j).unwrap();
    assert!(!r.auto_fetch_on_ready);
    assert_eq!(r.stale_threshold_seconds, 42);
    assert_eq!(r.worktree_dir.as_deref(), Some("/tmp/wt"));
    assert!(!r.protected_main);
}

#[test]
fn aborts_on_version_tracker_scope_field() {
    let j = r#"{"version": 1}"#;
    let e = RepoJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("tracker-scope") && s.contains("version"), "{s}");
}

#[test]
fn aborts_on_id_length_tracker_scope_field() {
    let j = r#"{"id_length": 4}"#;
    let e = RepoJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("tracker-scope") && s.contains("id_length"), "{s}");
}

#[test]
fn aborts_on_min_bl_version_tracker_scope_field() {
    let j = r#"{"min_bl_version": "0.4.0"}"#;
    let e = RepoJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(
        s.contains("tracker-scope") && s.contains("min_bl_version"),
        "{s}"
    );
}

#[test]
fn aborts_on_plugins_tracker_scope_field() {
    let j = r#"{"plugins": {}}"#;
    let e = RepoJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("tracker-scope") && s.contains("plugins"), "{s}");
}

#[test]
fn aborts_on_removed_target_branch_field() {
    // SPEC §6.7 / §14.18: `target_branch` was removed in bl-dfd1;
    // a repo.json shipping it aborts.
    let j = r#"{"target_branch": "develop"}"#;
    let e = RepoJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("target_branch") && s.contains("removed"), "{s}");
}

#[test]
fn lenient_on_truly_unknown_field() {
    // SPEC §6.9: lenient on unknown forward-compat fields. Only
    // the two trip-wire lists (tracker-scope, removed) abort.
    let j = r#"{"some_future_field": 42}"#;
    let r = RepoJson::from_json(j).unwrap();
    assert!(r.auto_fetch_on_ready); // defaults still applied
}

#[test]
fn reads_legacy_pre_xdg_repo_json_with_zero_keys() {
    // A repo.json from a freshly-migrated clone has no fields set
    // — the defaults handle the lookup. This is the SPEC §6.3
    // "ships a zero-keys repo.json and gets the defaults" case.
    let r = RepoJson::from_json("{}").unwrap();
    assert!(r.auto_fetch_on_ready);
}

#[test]
fn round_trip_json_preserves_explicit_layered_fields() {
    let mut r = RepoJson {
        integrate: Some(Integrate {
            mode: IntegrateMode::ForgePr,
        }),
        review: Some(ReviewBlock {
            gate_command: Some("make check".into()),
        }),
        require_remote_on_claim: Some(false),
        ..Default::default()
    };
    r.protected_main = false;
    let s = r.to_json().unwrap();
    let parsed = RepoJson::from_json(&s).unwrap();
    assert_eq!(r, parsed);
}

#[test]
fn round_trip_json_skips_none_layered_fields() {
    let r = RepoJson::default();
    let s = r.to_json().unwrap();
    assert!(!s.contains("integrate"), "{s}");
    assert!(!s.contains("review"), "{s}");
    assert!(!s.contains("require_remote_on_claim"), "{s}");
}

#[test]
fn read_or_default_returns_default_when_absent() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("nope.json");
    let r = RepoJson::read_or_default(&p).unwrap();
    assert_eq!(r, RepoJson::default());
}

#[test]
fn read_or_default_reads_existing_file() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("r.json");
    std::fs::write(&p, r#"{"protected_main": false}"#).unwrap();
    let r = RepoJson::read_or_default(&p).unwrap();
    assert!(!r.protected_main);
}

#[test]
fn read_or_default_propagates_non_notfound_io_error() {
    let td = tempfile::tempdir().unwrap();
    let r = RepoJson::read_or_default(td.path());
    assert!(matches!(r, Err(BallError::Io(_))));
}

#[test]
fn read_or_default_propagates_json_error_on_garbage() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("r.json");
    std::fs::write(&p, b"not json").unwrap();
    let e = RepoJson::read_or_default(&p).unwrap_err();
    assert!(matches!(e, BallError::Json(_)));
}

#[test]
fn check_forbidden_fields_skips_non_object() {
    // Defensive: a top-level array or string passes the check (the
    // parser would fail downstream). Don't crash here.
    let v: serde_json::Value = serde_json::from_str("[]").unwrap();
    assert!(check_forbidden_fields(&v, "x").is_ok());
}

#[test]
fn check_forbidden_fields_passes_clean_object() {
    let v: serde_json::Value =
        serde_json::from_str(r#"{"auto_fetch_on_ready": true}"#).unwrap();
    assert!(check_forbidden_fields(&v, "x").is_ok());
}

#[test]
fn check_forbidden_fields_names_the_file_label() {
    let v: serde_json::Value = serde_json::from_str(r#"{"version": 1}"#).unwrap();
    let e = check_forbidden_fields(&v, "clone.json").unwrap_err();
    assert!(format!("{e}").contains("clone.json"));
}
