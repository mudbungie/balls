use super::*;
use std::io::Write;

#[test]
fn parses_minimal_pointer_with_only_state_url() {
    let j = r#"{"state_url": "git@github.com:foo/bar.git"}"#;
    let t = TrackerJson::from_json(j).unwrap();
    assert_eq!(t.state_url, "git@github.com:foo/bar.git");
    assert_eq!(t.state_branch, None);
    assert_eq!(t.effective_branch(), DEFAULT_STATE_BRANCH);
}

#[test]
fn parses_pointer_with_explicit_branch() {
    let j = r#"{"state_url": "u", "state_branch": "shared/tasks"}"#;
    let t = TrackerJson::from_json(j).unwrap();
    assert_eq!(t.state_branch, Some("shared/tasks".into()));
    assert_eq!(t.effective_branch(), "shared/tasks");
}

#[test]
fn rejects_extra_field_per_14_6() {
    // SPEC §14.6: a tracker.json with any field other than
    // state_url/state_branch aborts read. `deny_unknown_fields` is
    // the serde gate.
    let j = r#"{"state_url": "u", "delivery": {"mode": "direct"}}"#;
    let e = TrackerJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("delivery") || s.contains("unknown"), "{s}");
}

#[test]
fn rejects_empty_state_url() {
    let j = r#"{"state_url": ""}"#;
    let e = TrackerJson::from_json(j).unwrap_err();
    assert!(format!("{e}").contains("redirect to nowhere"));
}

#[test]
fn rejects_missing_state_url() {
    let j = "{}";
    let e = TrackerJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("state_url") || s.contains("missing"), "{s}");
}

#[test]
fn rejects_missing_state_url_with_only_branch() {
    let j = r#"{"state_branch": "x"}"#;
    let e = TrackerJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("state_url") || s.contains("missing"), "{s}");
}

#[test]
fn round_trip_json_preserves_fields() {
    let t = TrackerJson {
        state_url: "u".into(),
        state_branch: Some("b".into()),
    };
    let s = t.to_json().unwrap();
    let parsed = TrackerJson::from_json(&s).unwrap();
    assert_eq!(t, parsed);
}

#[test]
fn round_trip_skips_none_branch() {
    let t = TrackerJson {
        state_url: "u".into(),
        state_branch: None,
    };
    let s = t.to_json().unwrap();
    assert!(!s.contains("state_branch"), "{s}");
    let parsed = TrackerJson::from_json(&s).unwrap();
    assert_eq!(t, parsed);
}

#[test]
fn read_optional_returns_none_when_file_absent() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("does-not-exist.json");
    let v = TrackerJson::read_optional(&p).unwrap();
    assert!(v.is_none());
}

#[test]
fn read_optional_returns_some_when_file_present() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("t.json");
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(br#"{"state_url": "url"}"#).unwrap();
    let v = TrackerJson::read_optional(&p).unwrap().unwrap();
    assert_eq!(v.state_url, "url");
}

#[test]
fn read_optional_propagates_io_error_on_unreadable() {
    // A path that exists but can't be opened — easiest portable
    // shape: pass a directory where a file is expected. `read_to_string`
    // returns IsADirectory (or PermissionDenied on some FS) which is
    // not NotFound, so we hit the `Err` branch.
    let td = tempfile::tempdir().unwrap();
    let v = TrackerJson::read_optional(td.path());
    assert!(v.is_err());
    let e = v.unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("io") || s.contains("directory") || s.contains("Is a"));
}

#[test]
fn read_optional_propagates_json_error_on_garbage() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("t.json");
    std::fs::write(&p, b"not json").unwrap();
    let e = TrackerJson::read_optional(&p).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("json") || s.contains("expected"));
}

#[test]
fn effective_branch_explicit_takes_precedence() {
    let t = TrackerJson {
        state_url: "u".into(),
        state_branch: Some("explicit".into()),
    };
    assert_eq!(t.effective_branch(), "explicit");
}

#[test]
fn rejects_state_url_as_non_string_type() {
    let j = r#"{"state_url": 42}"#;
    let e = TrackerJson::from_json(j).unwrap_err();
    let s = format!("{e}");
    assert!(s.contains("string") || s.contains("invalid"), "{s}");
}

#[test]
fn empty_state_branch_string_passes_but_resolves_to_empty() {
    // Edge: explicit empty branch is technically allowed by the
    // schema (Option<String>::Some("")). Document by example so a
    // future tightening is an intentional choice, not a silent
    // surprise.
    let j = r#"{"state_url": "u", "state_branch": ""}"#;
    let t = TrackerJson::from_json(j).unwrap();
    assert_eq!(t.effective_branch(), "");
}
