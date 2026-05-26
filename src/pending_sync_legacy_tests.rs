//! Unit tests for `count_files` — the helper that walks the legacy
//! `pending-sync/` tree and tallies on-disk staged reports. End-to-end
//! warning emission is covered by the integration test in
//! `tests/pending_sync_warning.rs`, since `warn_if_present` uses a
//! process-global `OnceLock` that other unit tests can race.

use super::count_files;
use std::fs;
use tempfile::TempDir;

#[test]
fn missing_root_returns_none() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(count_files(&tmp.path().join("does-not-exist")), None);
}

#[test]
fn empty_root_counts_zero() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("pending-sync");
    fs::create_dir_all(&root).unwrap();
    assert_eq!(count_files(&root), Some(0));
}

#[test]
fn flat_files_and_event_subdirs_both_tally() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("pending-sync");
    fs::create_dir_all(root.join("sync")).unwrap();
    fs::create_dir_all(root.join("close")).unwrap();
    fs::write(root.join("loose.json"), b"{}").unwrap();
    fs::write(root.join("sync").join("a.json"), b"{}").unwrap();
    fs::write(root.join("sync").join("b.json"), b"{}").unwrap();
    fs::write(root.join("close").join("c.json"), b"{}").unwrap();
    assert_eq!(count_files(&root), Some(4));
}
