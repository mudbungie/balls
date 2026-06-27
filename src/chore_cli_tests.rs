//! `Cli` seam tests — drive the real shelling against a fake `bl` script: a
//! zero-exit capture, a non-zero abort, and a missing binary (spawn failure).

#![cfg(unix)]

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// Write an executable fake `bl` (`/bin/sh` body) into `tmp`, return its path.
fn fake_bl(tmp: &TempDir, body: &str) -> PathBuf {
    let path = tmp.path().join("bl");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn a_zero_exit_returns_captured_stdout() {
    let tmp = TempDir::new().unwrap();
    let cli = Cli::at(fake_bl(&tmp, "echo '[]'"));
    let out = cli.run(tmp.path(), &["list".into(), "--json".into()]).unwrap();
    assert_eq!(out.trim(), "[]");
}

#[test]
fn a_non_zero_exit_is_an_error_naming_the_verb() {
    let tmp = TempDir::new().unwrap();
    let cli = Cli::at(fake_bl(&tmp, "echo 'nope' >&2; exit 1"));
    let err = cli.run(tmp.path(), &["create".into()]).unwrap_err();
    assert!(err.to_string().contains("bl create failed") && err.to_string().contains("nope"));
}

#[test]
fn a_missing_bl_binary_is_a_spawn_error() {
    let cli = Cli::at(PathBuf::from("/definitely/not/a/real/bl"));
    assert!(cli.run(Path::new("/"), &["list".into()]).is_err());
}
