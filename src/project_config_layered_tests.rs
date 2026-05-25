//! Tests for the SPEC-clone-layout §6.2 layered-field project-wide
//! defaults added to `ProjectConfig`. Split into a sibling test file
//! to keep `project_config_tests.rs` under the 300-line cap.

use super::*;
use crate::layered_fields::IntegrateMode;
use tempfile::TempDir;

fn write(dir: &TempDir, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn layered_defaults_absent_on_legacy_project_json() {
    // A pre-XDG project.json (with no `integrate`/`review` block)
    // reads cleanly — `Option<>` fields default to `None` so the
    // precedence merger falls through to the built-in default.
    let dir = TempDir::new().unwrap();
    let p = write(&dir, "p.json", r#"{"version": 1, "id_length": 4}"#);
    let cfg = ProjectConfig::load(&p).unwrap();
    assert_eq!(cfg.integrate, None);
    assert_eq!(cfg.review, None);
    assert_eq!(cfg.require_remote_on_claim, None);
    assert_eq!(cfg.require_remote_on_review, None);
    assert_eq!(cfg.require_remote_on_close, None);
}

#[test]
fn layered_defaults_carry_through_on_new_project_json() {
    let dir = TempDir::new().unwrap();
    let p = write(
        &dir,
        "p.json",
        r#"{
            "integrate": {"mode": "forge-pr"},
            "review": {"gate_command": "make check"},
            "require_remote_on_claim": false,
            "require_remote_on_review": true,
            "require_remote_on_close": true
        }"#,
    );
    let cfg = ProjectConfig::load(&p).unwrap();
    assert_eq!(cfg.integrate.unwrap().mode, IntegrateMode::ForgePr);
    assert_eq!(
        cfg.review.unwrap().gate_command.as_deref(),
        Some("make check")
    );
    assert_eq!(cfg.require_remote_on_claim, Some(false));
    assert_eq!(cfg.require_remote_on_review, Some(true));
    assert_eq!(cfg.require_remote_on_close, Some(true));
}

#[test]
fn layered_defaults_skipped_in_serialization_when_none() {
    // A default ProjectConfig writes without the new keys — keeps
    // pre-XDG project.json files byte-identical when no project-wide
    // layered default is set.
    let dir = TempDir::new().unwrap();
    let p = dir.path().join("p.json");
    ProjectConfig::default().save(&p).unwrap();
    let s = std::fs::read_to_string(&p).unwrap();
    assert!(!s.contains("integrate"), "{s}");
    assert!(!s.contains("require_remote_on_claim"), "{s}");
}
