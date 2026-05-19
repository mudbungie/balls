//! Ad-hoc plugin script builders, split from `plugin/mod.rs`. Each
//! installs a single-file `balls-plugin-mock` shell script with a
//! caller-supplied body/diagnostic — used to drive the runner's
//! graceful-degradation and diagnostics paths without a real plugin.
//! Re-exported from `plugin` so `common::plugin::install_plugin_*`
//! paths are unchanged.

#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;

/// Install a plugin script with a custom shell body. The body runs
/// when the plugin is invoked with `push` or `sync`; auth-check
/// passes iff `$AUTH_DIR/token.json` exists. Lower-level than
/// `install_plugin_with_body` — use this when you need to control
/// exactly what the plugin does (sleep, emit huge output, etc.).
pub fn install_plugin_script(body: &str) -> tempfile::TempDir {
    let bin_dir = tempfile::Builder::new()
        .prefix("balls-script-bin-")
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
{body}
        ;;
    *) exit 0 ;;
esac
"#
    );
    let path = bin_dir.path().join("balls-plugin-mock");
    fs::write(&path, script).unwrap();
    let mut p = fs::metadata(&path).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(&path, p).unwrap();
    bin_dir
}

/// Install a plugin that writes `diag_snippet` (verbatim POSIX sh) to
/// the diagnostics fd on push/sync, then returns an empty sync report.
pub fn install_plugin_with_diag(diag_snippet: &str) -> tempfile::TempDir {
    let bin_dir = tempfile::Builder::new().prefix("balls-diag-bin-").tempdir().unwrap();
    let script = format!(
        r#"#!/bin/sh
CMD="$1"; shift
AUTH_DIR=""
while [ $# -gt 0 ]; do
    case "$1" in
        --auth-dir) AUTH_DIR="$2"; shift 2 ;;
        *) shift ;;
    esac
done
case "$CMD" in
    auth-check) [ -f "$AUTH_DIR/token.json" ] && exit 0 || exit 1 ;;
    push|sync)
        {diag_snippet}
        cat - >/dev/null
        echo '{{"created":[],"updated":[],"deleted":[]}}'
        ;;
esac
"#
    );
    let path = bin_dir.path().join("balls-plugin-mock");
    fs::write(&path, script).unwrap();
    let mut p = fs::metadata(&path).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(&path, p).unwrap();
    bin_dir
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
