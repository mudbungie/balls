//! Unit tests for [`crate::speculate`] — fixture repos in tempdirs, no env
//! reads, no real toolchain (the toolchain string is an argument).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::{check, gate_fingerprint, read, record, verdict_path, worktree_oid, write, Verdict};

/// A throwaway git repo carrying the gate files the fingerprint reads.
fn repo() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(root.join("scripts")).unwrap();
    for rel in ["scripts/pre-commit", "scripts/check-line-lengths.sh", "scripts/check-coverage.sh"] {
        fs::write(root.join(rel), format!("#!/bin/sh\n# {rel}\n")).unwrap();
    }
    fs::write(root.join("Makefile"), "all:\n").unwrap();
    fs::write(root.join("code.rs"), "fn main() {}\n").unwrap();
    assert!(Command::new("git").arg("-C").arg(&root).arg("init").arg("-q").status().unwrap().success());
    (tmp, root)
}

fn scratch(tmp: &TempDir) -> PathBuf {
    tmp.path().join("scratch")
}

#[test]
fn worktree_oid_is_stable_and_content_sensitive() {
    let (tmp, root) = repo();
    let a = worktree_oid(&root, &scratch(&tmp)).unwrap();
    let b = worktree_oid(&root, &scratch(&tmp)).unwrap();
    assert_eq!(a, b, "same worktree, same tree");
    assert_eq!(a.len(), 40, "a full object id, not a truncation");
    fs::write(root.join("code.rs"), "fn main() { let _ = 1; }\n").unwrap();
    let c = worktree_oid(&root, &scratch(&tmp)).unwrap();
    assert_ne!(a, c, "an edited worktree is a different tree");
}

#[test]
fn worktree_oid_outside_a_repo_reports_gits_voice() {
    let tmp = TempDir::new().unwrap();
    let err = worktree_oid(tmp.path(), &scratch(&tmp)).unwrap_err();
    assert!(err.to_string().contains("git add -A"), "names the failing act: {err}");
}

#[test]
fn fingerprint_binds_toolchain_and_gate_files() {
    let (tmp, root) = repo();
    let a = gate_fingerprint(&root, &scratch(&tmp), "rustc 1.0").unwrap();
    assert_eq!(a, gate_fingerprint(&root, &scratch(&tmp), "rustc 1.0").unwrap());
    let b = gate_fingerprint(&root, &scratch(&tmp), "rustc 2.0").unwrap();
    assert_ne!(a, b, "a toolchain bump is a different gate");
    fs::write(root.join("Makefile"), "all: extra\n").unwrap();
    let c = gate_fingerprint(&root, &scratch(&tmp), "rustc 1.0").unwrap();
    assert_ne!(a, c, "an edited gate file is a different gate");
}

#[test]
fn fingerprint_requires_every_gate_file() {
    let (tmp, root) = repo();
    fs::remove_file(root.join("Makefile")).unwrap();
    assert!(gate_fingerprint(&root, &scratch(&tmp), "rustc 1.0").is_err());
}

#[test]
fn store_roundtrip_absence_and_corruption() {
    let tmp = TempDir::new().unwrap();
    let territory = tmp.path().join("territory");
    assert_eq!(read(&territory, "t", "g").unwrap(), None, "absence is a miss, not an error");
    let v = Verdict { pass: true, builder: "Gushed".into() };
    write(&territory, "t", "g", &v).unwrap();
    assert_eq!(read(&territory, "t", "g").unwrap(), Some(v));
    fs::write(verdict_path(&territory, "t", "g"), "not = [toml").unwrap();
    assert!(read(&territory, "t", "g").is_err(), "a corrupt record must not half-work");
    fs::create_dir_all(verdict_path(&territory, "dir", "key")).unwrap();
    assert!(read(&territory, "dir", "key").is_err(), "a non-NotFound IO failure surfaces");
}

#[test]
fn check_sees_only_a_pass_on_the_exact_tree() {
    let (tmp, root) = repo();
    let territory = tmp.path().join("territory");
    let s = scratch(&tmp);
    assert!(!check(&root, &s, &territory, "rustc 1.0").unwrap(), "empty store misses");
    record(&root, &s, &territory, "rustc 1.0", false, "Gushed").unwrap();
    assert!(!check(&root, &s, &territory, "rustc 1.0").unwrap(), "a recorded FAIL is not a pass");
    record(&root, &s, &territory, "rustc 1.0", true, "Gushed").unwrap();
    assert!(check(&root, &s, &territory, "rustc 1.0").unwrap(), "recorded pass hits");
    assert!(!check(&root, &s, &territory, "rustc 2.0").unwrap(), "another gate misses");
    fs::write(root.join("code.rs"), "fn main() { let _ = 2; }\n").unwrap();
    assert!(!check(&root, &s, &territory, "rustc 1.0").unwrap(), "another tree misses");
}

#[test]
fn scratch_holds_no_leftovers_after_any_call() {
    let (tmp, root) = repo();
    let s = scratch(&tmp);
    let territory = tmp.path().join("territory");
    record(&root, &s, &territory, "rustc 1.0", true, "Gushed").unwrap();
    check(&root, &s, &territory, "rustc 1.0").unwrap();
    let leftovers: Vec<_> = fs::read_dir(&s).unwrap().collect();
    assert!(leftovers.is_empty(), "scratch must be swept: {leftovers:?}");
}

/// The gate-blob scratch file is removed even when git cannot hash it — the
/// cleanup-invariant discipline the design's cleanup section demands.
#[test]
fn fingerprint_failure_still_sweeps_scratch() {
    let (tmp, root) = repo();
    let s = scratch(&tmp);
    fs::remove_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git"), "gitdir: /nonexistent\n").unwrap();
    assert!(gate_fingerprint(&root, &s, "rustc 1.0").is_err());
    assert!(!s.join("gate-blob").exists());
}

/// Path arithmetic is visible: the key IS the filename.
#[test]
fn verdict_path_is_the_key() {
    let p = verdict_path(Path::new("/t"), "abc", "def");
    assert_eq!(p, Path::new("/t/verdicts/abc-def.toml"));
}
