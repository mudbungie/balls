//! Unit tests for the bl-341b pending-sync doctor probe. Cover the
//! shape `bl sync --review` wrote before bl-6969: a `pending-sync/`
//! root with loose files plus one level of event subdirs.

use super::{count_files, finding};
use std::fs;
use tempfile::TempDir;

#[test]
fn missing_directory_yields_no_finding() {
    let tmp = TempDir::new().unwrap();
    assert!(finding(tmp.path()).is_none());
}

#[test]
fn empty_directory_yields_no_finding() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join(".balls/local/pending-sync")).unwrap();
    assert!(finding(tmp.path()).is_none());
}

#[test]
fn populated_directory_emits_count_and_path() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join(".balls/local/pending-sync");
    fs::create_dir_all(dir.join("sync")).unwrap();
    fs::create_dir_all(dir.join("close")).unwrap();
    fs::write(dir.join("loose.json"), b"{}").unwrap();
    fs::write(dir.join("sync").join("a.json"), b"{}").unwrap();
    fs::write(dir.join("sync").join("b.json"), b"{}").unwrap();
    fs::write(dir.join("close").join("c.json"), b"{}").unwrap();
    let f = finding(tmp.path()).expect("populated dir must surface a finding");
    assert!(f.problem.contains("4 staged sync reports"));
    assert!(f.problem.contains(".balls/local/pending-sync"));
    let hint = f.hint.as_deref().unwrap_or_default();
    assert!(hint.contains("bl-6969"));
    assert!(hint.contains("Remove the directory manually"));
}

#[test]
fn count_files_missing_root_is_none() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(count_files(&tmp.path().join("does-not-exist")), None);
}

#[test]
fn count_files_empty_root_is_zero() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("pending-sync");
    fs::create_dir_all(&root).unwrap();
    assert_eq!(count_files(&root), Some(0));
}
