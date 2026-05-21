//! bl-fb4d conformance, continued — `bl close` enforcement and the
//! SPEC §5.1 side channel. Split out of `plugin_participant_enforce.rs`
//! (which keeps the `bl update` policy matrix) to stay under the
//! 300-line cap.
//!
//! - a required `reject` on `close` aborts the command with the reason
//!   verbatim and rolls the state branch back so the task is not
//!   archived (§9, the ball's named target);
//! - a `wants_context` plugin sees `task_before` and `commit` on the
//!   §5.1 side channel (conformance #21).

mod common;

use common::native_plugin::{create_auth, install_native_plugin, jira_policy, path_with};
use common::*;
use std::os::unix::fs::PermissionsExt;

/// Install a hand-written `balls-plugin-jira` whose `propose` parses
/// `--ctx-file` (the shared harness drops that flag). Used by the
/// §5.1 side-channel test.
fn install_ctx_plugin(propose: &str) -> tempfile::TempDir {
    let dir = tempfile::Builder::new().prefix("balls-ctx-").tempdir().unwrap();
    let path = dir.path().join("balls-plugin-jira");
    let script = format!(
        r#"#!/bin/sh
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
  auth-setup) mkdir -p "$AUTH_DIR" && echo '{{"token":"t"}}' > "$AUTH_DIR/token.json"; exit 0 ;;
  describe) echo '{{"subscriptions":["update"],"projection":{{"external_prefixes":["jira"]}},"wants_context":true}}'; exit 0 ;;
  propose) {propose} ;;
  *) exit 1 ;;
esac
"#
    );
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();
    dir
}

#[test]
fn required_reject_aborts_close_and_unarchives() {
    // The ball's named target: a required veto on `bl close` aborts
    // with the reason and rolls the state branch back so the task is
    // not archived. Reject only on `close`; claim/review accept.
    let bin = install_native_plugin(
        "jira",
        r#"
        cat - >/dev/null
        if [ "$EVENT" = close ]; then
            printf '{"reject":{"reason":"close blocked: ticket open"}}\n'
        else
            printf '{"ok":{"task":{}}}\n'
        fi
        exit 0
        "#,
    );
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "to-close");
    jira_policy(repo.path(), "close", "required");
    create_auth(repo.path(), "jira");
    let env_path = path_with(&[bin.path()]);

    bl(repo.path())
        .env("PATH", &env_path)
        .args(["claim", &id])
        .assert()
        .success();
    bl(repo.path())
        .env("PATH", &env_path)
        .args(["review", &id, "-m", "done"])
        .assert()
        .success();
    let out = bl(repo.path())
        .env("PATH", &env_path)
        .args(["close", &id, "-m", "ship"])
        .output()
        .unwrap();

    assert!(!out.status.success(), "required veto must abort bl close");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("close blocked: ticket open"),
        "close reason verbatim: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task = read_task_json(repo.path(), &id);
    assert_eq!(
        task["status"], "review",
        "the close+archive commit must be rewound: {task}"
    );
}

#[test]
fn context_aware_plugin_sees_task_before_and_commit() {
    // The §5.1 side channel must carry the bl-fb4d additive keys:
    // `task_before` (pre-image diff basis) and `commit` (the event's
    // state-branch sha). The plugin reports whether each is present.
    let bin = install_ctx_plugin(
        r#"
        cat - >/dev/null
        if [ -z "$CTX" ] || [ ! -f "$CTX" ]; then echo "ctx missing" >&2; exit 1; fi
        B=0; C=0
        grep -q '"task_before"' "$CTX" && B=1
        grep -q '"commit"' "$CTX" && C=1
        printf '{"ok":{"task":{"external":{"jira":{"b":%s,"c":%s}}}}}\n' "$B" "$C"
        exit 0
        "#,
    );
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "ctx");
    jira_policy(repo.path(), "update", "best-effort");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--note", "poke"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "update failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let jira = read_task_json(repo.path(), &id)["external"]["jira"].clone();
    assert_eq!(jira["b"], 1, "task_before delivered on §5.1 channel: {jira}");
    assert_eq!(jira["c"], 1, "commit sha delivered on §5.1 channel: {jira}");
}
