use super::*;
use std::fs;
use tempfile::TempDir;

fn tmp() -> TempDir {
    tempfile::Builder::new().prefix("balls-legacy-").tempdir().unwrap()
}

#[test]
fn empty_clone_has_no_markers() {
    let dir = tmp();
    assert!(detect(dir.path()).is_empty());
    assert!(!is_legacy(dir.path()));
}

#[test]
fn pre_xdg_config_json_is_a_marker() {
    let dir = tmp();
    fs::create_dir_all(dir.path().join(".balls")).unwrap();
    fs::write(dir.path().join(".balls/config.json"), "{}").unwrap();
    let markers = detect(dir.path());
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].kind, "pre-XDG .balls/config.json on the code branch");
    assert_eq!(markers[0].path, dir.path().join(".balls/config.json"));
    assert!(is_legacy(dir.path()));
}

#[test]
fn warning_line_names_marker_paths_and_suggests_prime_migrate() {
    let dir = tmp();
    fs::create_dir_all(dir.path().join(".balls")).unwrap();
    fs::write(dir.path().join(".balls/config.json"), "{}").unwrap();
    let line = warning_line(&detect(dir.path()));
    assert!(line.contains("legacy layout in use"));
    assert!(line.contains(".balls/config.json"));
    assert!(line.contains("bl prime --migrate"));
    assert!(line.contains("bl migrate"));
}

#[test]
fn warning_line_with_no_markers_is_empty_shaped() {
    // Callers gate on is_legacy() before calling; this just exercises
    // the join-of-empty arm so coverage stays 100%.
    let line = warning_line(&[]);
    assert!(line.contains("legacy layout in use ()"));
}
