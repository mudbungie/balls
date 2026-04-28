//! Shared fixtures for integration tests of the bl-8b71 native
//! participant protocol. Two test files install plugins, write a
//! config that subscribes them, and seed auth tokens; this module
//! collects the boilerplate so the test bodies stay focused on the
//! scenarios they verify.

#![allow(dead_code)]

use super::git;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Install a native plugin script at `bin_dir/balls-plugin-<name>`.
/// `propose_body` is plain POSIX sh and runs inside the `propose`
/// case arm with `$EVENT`, `$STATE_DIR` (auth dir), and stdin (the
/// Task JSON) in scope. The body is responsible for printing the
/// JSON propose response on stdout.
pub fn install_native_plugin(name: &str, propose_body: &str) -> tempfile::TempDir {
    let bin_dir = tempfile::Builder::new()
        .prefix(&format!("balls-native-{name}-"))
        .tempdir()
        .unwrap();
    let script = format!(
        r#"#!/bin/sh
CMD="$1"; shift
AUTH_DIR=""
EVENT=""
while [ $# -gt 0 ]; do
    case "$1" in
        --auth-dir) AUTH_DIR="$2"; shift 2 ;;
        --event) EVENT="$2"; shift 2 ;;
        *) shift ;;
    esac
done
case "$CMD" in
    auth-check) [ -f "$AUTH_DIR/token.json" ] && exit 0 || exit 1 ;;
    auth-setup)
        mkdir -p "$AUTH_DIR" && echo '{{"token":"t"}}' > "$AUTH_DIR/token.json"
        exit 0
        ;;
    describe)
        cat <<'JSON'
{{ "subscriptions": ["claim", "review", "close", "update"],
   "projection": {{ "external_prefixes": ["{name}"] }} }}
JSON
        exit 0
        ;;
    propose)
        STATE_DIR="$AUTH_DIR"
        {propose_body}
        ;;
    *) echo "unknown subcommand: $CMD" >&2; exit 1 ;;
esac
"#,
    );
    write_script(&bin_dir, name, &script);
    bin_dir
}

/// Install a minimal legacy plugin (push/sync only) — a sibling of
/// `install_native_plugin` for mixed-config tests. Its `describe`
/// exits non-zero so the dispatcher routes it through the legacy
/// shim per SPEC §12.
pub fn install_legacy_plugin(name: &str) -> tempfile::TempDir {
    let bin_dir = tempfile::Builder::new()
        .prefix(&format!("balls-legacy-{name}-"))
        .tempdir()
        .unwrap();
    let script = r#"#!/bin/sh
CMD="$1"; shift
AUTH_DIR=""
TASK_ID=""
while [ $# -gt 0 ]; do
    case "$1" in
        --auth-dir) AUTH_DIR="$2"; shift 2 ;;
        --task) TASK_ID="$2"; shift 2 ;;
        *) shift ;;
    esac
done
case "$CMD" in
    auth-check) [ -f "$AUTH_DIR/token.json" ] && exit 0 || exit 1 ;;
    auth-setup) mkdir -p "$AUTH_DIR" && echo '{"token":"t"}' > "$AUTH_DIR/token.json"; exit 0 ;;
    push) cat - >/dev/null; printf '{"remote_key":"LEGACY-%s"}\n' "$TASK_ID"; exit 0 ;;
    sync) cat - >/dev/null; echo '{"created":[],"updated":[],"deleted":[]}'; exit 0 ;;
    describe) exit 1 ;;
    propose) exit 1 ;;
    *) exit 1 ;;
esac
"#;
    write_script(&bin_dir, name, script);
    bin_dir
}

fn write_script(bin_dir: &tempfile::TempDir, name: &str, body: &str) {
    let path = bin_dir.path().join(format!("balls-plugin-{name}"));
    fs::write(&path, body).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
}

/// Write a `.balls/config.json` that enables every named plugin and
/// commits the result. Mirrors what `bl init` plus a hand-edit would
/// produce; tests use it to wire up multiple plugins in one shot.
pub fn write_plugin_config(repo: &Path, names: &[&str]) {
    let plugins_dir = repo.join(".balls/plugins");
    fs::create_dir_all(&plugins_dir).unwrap();
    for n in names {
        fs::write(plugins_dir.join(format!("{n}.json")), "{}").unwrap();
    }
    let cfg_path = repo.join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    let mut plugins = serde_json::Map::new();
    for n in names {
        plugins.insert(
            (*n).to_string(),
            serde_json::json!({
                "enabled": true,
                "sync_on_change": true,
                "config_file": format!(".balls/plugins/{n}.json"),
            }),
        );
    }
    cfg["plugins"] = serde_json::Value::Object(plugins);
    fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
    git(repo, &["add", ".balls/config.json", ".balls/plugins"]);
    git(repo, &["commit", "-m", "configure plugins", "--no-verify"]);
}

pub fn create_auth(repo: &Path, name: &str) {
    let auth_dir = repo.join(format!(".balls/local/plugins/{name}"));
    fs::create_dir_all(&auth_dir).unwrap();
    fs::write(auth_dir.join("token.json"), r#"{"token":"t"}"#).unwrap();
}

pub fn path_with(bins: &[&Path]) -> String {
    let mut parts: Vec<String> = bins.iter().map(|p| p.display().to_string()).collect();
    parts.push(std::env::var("PATH").unwrap_or_default());
    parts.join(":")
}
