//! Phase 5: plugin system. We use a shell script mock plugin that records
//! invocations and responds with canned output.
//!
//! Covers user stories 64–72.

mod common;

use common::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

/// Install a mock `ball-plugin-mock` script into a temp bin dir and return the
/// bin dir path for PATH injection.
fn install_mock_plugin() -> (tempfile::TempDir, std::path::PathBuf) {
    let bin_dir = tempfile::Builder::new()
        .prefix("ball-mock-bin-")
        .tempdir()
        .unwrap();
    let log_file = bin_dir.path().join("calls.log");
    let script = format!(
        r#"#!/bin/sh
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $@" >> "{log}"
if [ "$1" = "auth-check" ]; then exit 0; fi
if [ "$1" = "push" ]; then
  cat - >> "{log}.stdin" 2>/dev/null || true
  exit 0
fi
if [ "$1" = "sync" ]; then
  echo "{{\"changed\": []}}"
  exit 0
fi
if [ "$1" = "pull" ]; then
  echo "[]"
  exit 0
fi
exit 0
"#,
        log = log_file.display()
    );
    let script_path = bin_dir.path().join("ball-plugin-mock");
    fs::write(&script_path, script).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();
    let log = log_file;
    (bin_dir, log)
}

fn configure_plugin(repo_path: &std::path::Path) {
    let config_dir = repo_path.join(".ball/plugins");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("mock.json"),
        r#"{"url":"https://mock.example"}"#,
    )
    .unwrap();

    let cfg_path = repo_path.join(".ball/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["plugins"] = serde_json::json!({
        "mock": {
            "enabled": true,
            "sync_on_change": true,
            "config_file": ".ball/plugins/mock.json"
        }
    });
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
}

#[test]
fn story_65_create_triggers_plugin_push() {
    let (bin_dir, log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());

    bl(repo.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .args(["create", "with plugin"])
        .assert()
        .success();

    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        log_contents.contains("push"),
        "expected plugin push call in log: {}",
        log_contents
    );
}

#[test]
fn story_67_sync_triggers_plugin_sync() {
    let (bin_dir, log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());

    bl(repo.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .arg("sync")
        .assert()
        .success();

    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    assert!(log_contents.contains("sync"));
}

#[test]
fn story_71_plugin_unavailable_does_not_block_sync() {
    // No mock plugin installed, but config references one — sync should
    // warn and succeed anyway.
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    bl(repo.path())
        .env("PATH", "/usr/bin:/bin")
        .arg("sync")
        .assert()
        .success();
}

#[test]
fn plugin_unavailable_does_not_block_create() {
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    bl(repo.path())
        .env("PATH", "/usr/bin:/bin")
        .args(["create", "no plugin"])
        .assert()
        .success();
}

/// Install a mock plugin that always fails (non-zero exit).
fn install_failing_plugin() -> (tempfile::TempDir, std::path::PathBuf) {
    let bin_dir = tempfile::Builder::new()
        .prefix("ball-failing-bin-")
        .tempdir()
        .unwrap();
    let log_file = bin_dir.path().join("calls.log");
    let script = format!(
        r#"#!/bin/sh
echo "$@" >> "{log}"
echo "plugin failure" 1>&2
exit 1
"#,
        log = log_file.display()
    );
    let path = bin_dir.path().join("ball-plugin-mock");
    fs::write(&path, script).unwrap();
    let mut p = fs::metadata(&path).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(&path, p).unwrap();
    (bin_dir, log_file)
}

#[test]
fn plugin_push_failure_is_warned_not_fatal() {
    let (bin_dir, log) = install_failing_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());

    bl(repo.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .args(["create", "failing plugin"])
        .assert()
        .success();
    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    assert!(log_contents.contains("push"));
}

#[test]
fn plugin_sync_failure_is_warned_not_fatal() {
    let (bin_dir, log) = install_failing_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());

    bl(repo.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                bin_dir.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .arg("sync")
        .assert()
        .success();
    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    assert!(log_contents.contains("sync"));
}
