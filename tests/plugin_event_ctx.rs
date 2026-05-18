//! SPEC §5.1 / §17.21 conformance (bl-bac2), end-to-end.
//!
//! - A native plugin that declares `wants_context: true` receives the
//!   EventCtx document via `--ctx-file`: it can read the actor, the
//!   event, and the pinned schema_version that the bare post-image
//!   Task on stdin cannot tell it.
//! - A plugin that does NOT declare it is never passed `--ctx-file`
//!   and is byte-identical to today (the fixture hard-fails if it
//!   ever sees the flag, so a regression is caught, not silently
//!   tolerated).
//! - Forward-compat: the context-aware fixture extracts only the keys
//!   it knows and ignores the rest — an additive future key cannot
//!   break it (SPEC §13 / §5.1).

mod common;

use common::native_plugin::{create_auth, path_with, write_plugin_config};
use common::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn install(script: &str) -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("balls-ctx-")
        .tempdir()
        .unwrap();
    let path = dir.path().join("balls-plugin-jira");
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    dir
}

fn run_update(repo: &Path, bin: &Path, id: &str) -> std::process::Output {
    bl(repo)
        .env("PATH", path_with(&[bin]))
        .args(["update", id, "--note", "poke"])
        .output()
        .unwrap()
}

// Captures `--ctx-file`, refuses to run without it, and echoes the
// fields it understands back through its owned slice.
const WANTS_CTX: &str = r#"#!/bin/sh
CMD="$1"; shift
AUTH_DIR=""; CTX=""
while [ $# -gt 0 ]; do
  case "$1" in
    --auth-dir) AUTH_DIR="$2"; shift 2 ;;
    --ctx-file) CTX="$2"; shift 2 ;;
    *) shift ;;
  esac
done
case "$CMD" in
  auth-check) [ -f "$AUTH_DIR/token.json" ] && exit 0 || exit 1 ;;
  auth-setup) mkdir -p "$AUTH_DIR" && echo '{"token":"t"}' > "$AUTH_DIR/token.json"; exit 0 ;;
  describe) echo '{"subscriptions":["update"],"projection":{"external_prefixes":["jira"]},"wants_context":true}'; exit 0 ;;
  propose)
    cat - >/dev/null
    if [ -z "$CTX" ] || [ ! -f "$CTX" ]; then echo "ctx file missing" >&2; exit 1; fi
    A=$(sed -n 's/.*"actor":"\([^"]*\)".*/\1/p' "$CTX")
    S=$(sed -n 's/.*"schema_version":\([0-9]*\).*/\1/p' "$CTX")
    E=$(sed -n 's/.*"event":"\([^"]*\)".*/\1/p' "$CTX")
    printf '{"ok":{"task":{"external":{"jira":{"actor":"%s","sv":%s,"ev":"%s"}}}}}\n' "$A" "$S" "$E"
    exit 0 ;;
  *) exit 1 ;;
esac
"#;

// No `wants_context`; hard-fails if a `--ctx-file` is ever passed.
const NO_CTX: &str = r#"#!/bin/sh
CMD="$1"; shift
AUTH_DIR=""; SAW_CTX=""
while [ $# -gt 0 ]; do
  case "$1" in
    --auth-dir) AUTH_DIR="$2"; shift 2 ;;
    --ctx-file) SAW_CTX=1; shift 2 ;;
    *) shift ;;
  esac
done
case "$CMD" in
  auth-check) [ -f "$AUTH_DIR/token.json" ] && exit 0 || exit 1 ;;
  auth-setup) mkdir -p "$AUTH_DIR" && echo '{"token":"t"}' > "$AUTH_DIR/token.json"; exit 0 ;;
  describe) echo '{"subscriptions":["update"],"projection":{"external_prefixes":["jira"]}}'; exit 0 ;;
  propose)
    cat - >/dev/null
    if [ -n "$SAW_CTX" ]; then echo "unexpected --ctx-file" >&2; exit 1; fi
    printf '{"ok":{"task":{"external":{"jira":{"ok":"yes"}}}}}\n'
    exit 0 ;;
  *) exit 1 ;;
esac
"#;

#[test]
fn context_aware_plugin_receives_eventctx_via_ctx_file() {
    let bin = install(WANTS_CTX);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "ctx-aware");
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");

    let out = run_update(repo.path(), bin.path(), &id);
    assert!(
        out.status.success(),
        "update failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let jira = read_task_json(repo.path(), &id)["external"]["jira"].clone();
    assert_eq!(jira["actor"], "test-user", "EventCtx.actor delivered: {jira}");
    assert_eq!(jira["sv"], 1, "schema_version pinned at 1: {jira}");
    assert_eq!(jira["ev"], "update", "EventCtx.event delivered: {jira}");
}

#[test]
fn plugin_without_wants_context_is_never_passed_ctx_file() {
    let bin = install(NO_CTX);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "no-ctx");
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");

    let out = run_update(repo.path(), bin.path(), &id);
    assert!(
        out.status.success(),
        "byte-identical path must not pass --ctx-file: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        read_task_json(repo.path(), &id)["external"]["jira"]["ok"],
        "yes",
        "the non-context plugin still ran on its unchanged stdin wire"
    );
}
