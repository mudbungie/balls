//! Shared helpers for plugin integration tests.
//!
//! Provides mock plugin installation, configuration, and authentication.

#![allow(dead_code)]

use super::git;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Install a full-protocol `balls-plugin-mock` script into a temp bin dir.
/// Returns (bin_dir, log_path).
///
/// The plugin implements:
///   auth-check  — exit 0 if $AUTH_DIR/token.json exists, else exit 1
///   auth-setup  — creates $AUTH_DIR/token.json
///   push        — reads task JSON from stdin, writes PushResponse to stdout
///   sync        — reads tasks from stdin, returns canned SyncReport or empty
pub fn install_mock_plugin() -> (tempfile::TempDir, std::path::PathBuf) {
    let bin_dir = tempfile::Builder::new()
        .prefix("balls-mock-bin-")
        .tempdir()
        .unwrap();
    let log_file = bin_dir.path().join("calls.log");
    let script = format!(
        r#"#!/bin/sh
# Parse args
CMD="$1"
shift
AUTH_DIR=""
CONFIG=""
TASK_ID=""
while [ $# -gt 0 ]; do
    case "$1" in
        --auth-dir) AUTH_DIR="$2"; shift 2 ;;
        --config) CONFIG="$2"; shift 2 ;;
        --task) TASK_ID="$2"; shift 2 ;;
        *) shift ;;
    esac
done

echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $CMD task=$TASK_ID" >> "{log}"

case "$CMD" in
    auth-check)
        if [ -f "$AUTH_DIR/token.json" ]; then
            exit 0
        else
            echo "auth expired" >&2
            exit 1
        fi
        ;;
    auth-setup)
        mkdir -p "$AUTH_DIR"
        echo '{{"token":"mock-token"}}' > "$AUTH_DIR/token.json"
        exit 0
        ;;
    push)
        # Read stdin (task JSON), log it
        STDIN=$(cat -)
        echo "$STDIN" >> "{log}.stdin"
        # Return a PushResponse with remote_key derived from task ID
        if [ -n "$TASK_ID" ]; then
            printf '{{"remote_key":"MOCK-%s","remote_url":"https://mock.example/MOCK-%s","synced_at":"2026-01-01T00:00:00Z"}}\n' "$TASK_ID" "$TASK_ID"
        fi
        exit 0
        ;;
    sync)
        # Read stdin (all tasks JSON), log it
        STDIN=$(cat -)
        echo "$STDIN" >> "{log}.sync-stdin"
        # If a canned response file exists, return it
        if [ -f "$CONFIG.sync-response" ]; then
            cat "$CONFIG.sync-response"
        else
            echo '{{"created":[],"updated":[],"deleted":[]}}'
        fi
        exit 0
        ;;
    *)
        echo "unknown command: $CMD" >&2
        exit 1
        ;;
esac
"#,
        log = log_file.display()
    );
    let script_path = bin_dir.path().join("balls-plugin-mock");
    fs::write(&script_path, script).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();
    (bin_dir, log_file)
}

pub fn configure_plugin(repo_path: &Path) {
    let config_dir = repo_path.join(".balls/plugins");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("mock.json"),
        r#"{"url":"https://mock.example"}"#,
    )
    .unwrap();

    let cfg_path = repo_path.join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["plugins"] = serde_json::json!({
        "mock": {
            "enabled": true,
            "sync_on_change": true,
            "config_file": ".balls/plugins/mock.json"
        }
    });
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    // Commit config changes so worktrees created later include them.
    git(repo_path, &["add", ".balls/config.json", ".balls/plugins/mock.json"]);
    git(repo_path, &["commit", "-m", "configure mock plugin", "--no-verify"]);
}

/// Create the mock auth token so auth-check passes.
pub fn create_mock_auth(repo_path: &Path) {
    let auth_dir = repo_path.join(".balls/local/plugins/mock");
    fs::create_dir_all(&auth_dir).unwrap();
    fs::write(auth_dir.join("token.json"), r#"{"token":"mock-token"}"#).unwrap();
}

pub fn path_with_mock(bin_dir: &Path) -> String {
    format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

/// Install a mock plugin that passes auth-check but fails on push/sync.
pub fn install_failing_plugin() -> (tempfile::TempDir, std::path::PathBuf) {
    let bin_dir = tempfile::Builder::new()
        .prefix("balls-failing-bin-")
        .tempdir()
        .unwrap();
    let log_file = bin_dir.path().join("calls.log");
    let script = format!(
        r#"#!/bin/sh
CMD="$1"
shift
AUTH_DIR=""
while [ $# -gt 0 ]; do
    case "$1" in
        --auth-dir) AUTH_DIR="$2"; shift 2 ;;
        *) shift ;;
    esac
done
echo "$CMD $@" >> "{log}"
if [ "$CMD" = "auth-check" ]; then
    [ -f "$AUTH_DIR/token.json" ] && exit 0 || exit 1
fi
echo "plugin failure" 1>&2
exit 1
"#,
        log = log_file.display()
    );
    let path = bin_dir.path().join("balls-plugin-mock");
    fs::write(&path, script).unwrap();
    let mut p = fs::metadata(&path).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(&path, p).unwrap();
    (bin_dir, log_file)
}

pub fn write_sync_response(repo_path: &Path, response: &str) {
    let response_path = repo_path.join(".balls/plugins/mock.json.sync-response");
    fs::write(response_path, response).unwrap();
}

/// Mock plugin that passes auth-check but returns the provided body on
/// push/sync (empty, invalid JSON, etc.). Used to exercise plugin
/// runner's graceful-degradation paths.
pub fn install_plugin_with_body(body: &str) -> tempfile::TempDir {
    let bin_dir = tempfile::Builder::new()
        .prefix("balls-body-bin-")
        .tempdir()
        .unwrap();
    let script = format!(
        r#"#!/bin/sh
CMD="$1"
shift
AUTH_DIR=""
while [ $# -gt 0 ]; do
    case "$1" in
        --auth-dir) AUTH_DIR="$2"; shift 2 ;;
        *) shift ;;
    esac
done
case "$CMD" in
    auth-check)
        [ -f "$AUTH_DIR/token.json" ] && exit 0 || exit 1
        ;;
    push|sync)
        cat - >/dev/null
        printf '%s' '{body}'
        exit 0
        ;;
    *) exit 0 ;;
esac
"#,
        body = body.replace('\'', "'\\''")
    );
    let path = bin_dir.path().join("balls-plugin-mock");
    fs::write(&path, script).unwrap();
    let mut p = fs::metadata(&path).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(&path, p).unwrap();
    bin_dir
}
