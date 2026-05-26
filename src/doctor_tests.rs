//! Unit tests for `doctor.rs`. Split out to keep the host file under
//! the 300-line cap; functions stay private to the parent module and
//! are reached via `super::` (the standard `#[path]` test-module
//! pattern shared with `xdg_discover_tests`, `policy_tests`, etc.).

use super::{check_tasks_dir_override, legacy_layout_finding};
use crate::clone_json::CloneJson;
use crate::store::{Layout, Store};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn synth_xdg_store(root: PathBuf, clone_json: Option<CloneJson>) -> Store {
    Store {
        root,
        stealth: true,
        no_git: false,
        layout: Layout::Xdg,
        tasks_dir_path: PathBuf::new(),
        state_repo_path: PathBuf::new(),
        state_branch_name: String::new(),
        claims_dir_path: PathBuf::new(),
        lock_dir_path: PathBuf::new(),
        local_plugins_dir_path: PathBuf::new(),
        worktrees_root_path: PathBuf::new(),
        config_file_path: PathBuf::new(),
        project_config_file_path: PathBuf::new(),
        clone_json,
    }
}

#[test]
fn returns_none_when_no_markers() {
    let dir = TempDir::new().unwrap();
    assert!(legacy_layout_finding(dir.path()).is_none());
}

#[test]
fn returns_some_when_marker_present() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(".balls")).unwrap();
    fs::write(dir.path().join(".balls/config.json"), "{}").unwrap();
    let f = legacy_layout_finding(dir.path()).unwrap();
    assert!(f.problem.contains("legacy layout in use"));
}

#[test]
fn check_tasks_dir_override_flags_missing_path() {
    let dir = TempDir::new().unwrap();
    let cj = CloneJson {
        stealth: true,
        tasks_dir: Some("/no/such/path/at/all".into()),
        ..Default::default()
    };
    let store = synth_xdg_store(dir.path().to_path_buf(), Some(cj));
    let mut out = Vec::new();
    check_tasks_dir_override(&store, &mut out);
    assert_eq!(out.len(), 1, "expected one finding");
    assert!(out[0].problem.contains("clone.json tasks_dir"));
    assert!(out[0].problem.contains("/no/such/path/at/all"));
    assert!(out[0].hint.as_deref().is_some_and(|h| h.contains("clone.json")));
}

#[test]
fn check_tasks_dir_override_silent_when_path_exists() {
    let dir = TempDir::new().unwrap();
    let cj = CloneJson {
        stealth: true,
        tasks_dir: Some(dir.path().to_string_lossy().into_owned()),
        ..Default::default()
    };
    let store = synth_xdg_store(dir.path().to_path_buf(), Some(cj));
    let mut out = Vec::new();
    check_tasks_dir_override(&store, &mut out);
    assert!(out.is_empty(), "existing path → no finding");
}

#[test]
fn check_tasks_dir_override_silent_when_no_tasks_dir() {
    // `clone.json` present but `tasks_dir` absent (non-stealth case
    // with layered overrides only) — nothing to check.
    let dir = TempDir::new().unwrap();
    let cj = CloneJson {
        require_remote_on_claim: Some(true),
        ..Default::default()
    };
    let store = synth_xdg_store(dir.path().to_path_buf(), Some(cj));
    let mut out = Vec::new();
    check_tasks_dir_override(&store, &mut out);
    assert!(out.is_empty());
}

#[test]
fn check_tasks_dir_override_silent_when_no_clone_json() {
    // Legacy layout (no clone.json) → early return without
    // attempting to read any field.
    let dir = TempDir::new().unwrap();
    let store = synth_xdg_store(dir.path().to_path_buf(), None);
    let mut out = Vec::new();
    check_tasks_dir_override(&store, &mut out);
    assert!(out.is_empty());
}
