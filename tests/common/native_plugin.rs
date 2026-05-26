//! Shared fixtures for integration tests of the bl-8b71 native
//! participant protocol. Two test files install plugins, write a
//! config that subscribes them, and seed auth tokens; this module
//! collects the boilerplate so the test bodies stay focused on the
//! scenarios they verify.

#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Install a native plugin script at `bin_dir/balls-plugin-<name>`.
/// `propose_body` is plain POSIX sh and runs inside the `propose`
/// case arm with `$EVENT`, `$STATE_DIR` (auth dir), and stdin (the
/// Task JSON) in scope. The body is responsible for printing the
/// JSON propose response on stdout.
pub fn install_native_plugin(name: &str, propose_body: &str) -> tempfile::TempDir {
    let describe = format!(
        r#"{{ "subscriptions": ["create", "claim", "review", "close", "update"],
   "projection": {{ "external_prefixes": ["{name}"] }} }}"#
    );
    install_native_plugin_describe(name, &describe, propose_body)
}

/// Like `install_native_plugin` but with a caller-supplied `describe`
/// payload. Used by forward-compat conformance (a describe that
/// subscribes to an event this build does not know) and by siblings
/// that need to declare new describe fields. `describe_json` is the
/// exact JSON the `describe` subcommand prints.
pub fn install_native_plugin_describe(
    name: &str,
    describe_json: &str,
    propose_body: &str,
) -> tempfile::TempDir {
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
{describe_json}
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
    describe) printf "error: unrecognized subcommand 'describe'\n" >&2; exit 2 ;;
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

/// Set the project `plugins` map (`.balls/project.json`, SPEC §7) so
/// every named plugin is enabled, and commit the result on the tracker
/// branch. Mirrors what `bl init` plus a hand-edit would produce;
/// tests use it to wire up multiple plugins in one shot.
pub fn write_plugin_config(repo: &Path, names: &[&str]) {
    let plugins_dir = repo.join(".balls/plugins");
    fs::create_dir_all(&plugins_dir).unwrap();
    for n in names {
        fs::write(plugins_dir.join(format!("{n}.json")), "{}").unwrap();
    }
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
    super::set_project_plugins(repo, serde_json::Value::Object(plugins));
    super::commit_state_repo(repo, "configure plugins");
}

/// Write a config subscribing `jira` to one lifecycle event with one
/// policy, and commit it. The participant-enforcement tests use this
/// to wire a single plugin with a chosen `required`/`best-effort`/
/// `gating` policy in one call.
pub fn jira_policy(repo: &Path, event: &str, policy: &str) {
    let plugins_dir = repo.join(".balls/plugins");
    fs::create_dir_all(&plugins_dir).unwrap();
    fs::write(plugins_dir.join("jira.json"), "{}").unwrap();
    super::set_project_plugins(
        repo,
        serde_json::json!({
            "jira": {
                "enabled": true,
                "sync_on_change": false,
                "config_file": ".balls/plugins/jira.json",
                "participant": { "subscriptions": { event: { "policy": policy } } }
            }
        }),
    );
    super::commit_state_repo(repo, "configure jira");
}

pub fn create_auth(repo: &Path, name: &str) {
    let auth_dir = super::plugins_auth_dir(repo).join(name);
    fs::create_dir_all(&auth_dir).unwrap();
    fs::write(auth_dir.join("token.json"), r#"{"token":"t"}"#).unwrap();
}

pub fn path_with(bins: &[&Path]) -> String {
    let mut parts: Vec<String> = bins.iter().map(|p| p.display().to_string()).collect();
    parts.push(std::env::var("PATH").unwrap_or_default());
    parts.join(":")
}
