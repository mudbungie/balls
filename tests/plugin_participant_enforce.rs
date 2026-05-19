//! bl-fb4d conformance — command-level participant enforcement.
//!
//! SPEC §9/§8.1/§11/§5.1, end-to-end through the real binary. The
//! negotiation primitive computes a required `reject` correctly
//! (pinned in `src/negotiation_reject_tests.rs`); this file proves
//! the *command* now consumes it:
//!
//! - required `reject` aborts `bl close`/`bl update` with the reason
//!   verbatim and rolls the state branch back (§9, conformance #19);
//! - best-effort `reject` ships and records `task.sync_status.<p>`;
//! - `--skip=NAME` ships past a required veto and logs `[--skip=NAME]`
//!   in the state-branch commit subject (§11, conformance #9);
//! - a `wants_context` plugin sees `task_before` and `commit` on the
//!   §5.1 side channel (conformance #21).

mod common;

use common::native_plugin::{create_auth, install_native_plugin, path_with};
use common::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

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

/// Write a config subscribing `jira` to one event with one policy.
fn jira_policy(repo: &Path, event: &str, policy: &str) {
    let plugins_dir = repo.join(".balls/plugins");
    std::fs::create_dir_all(&plugins_dir).unwrap();
    std::fs::write(plugins_dir.join("jira.json"), "{}").unwrap();
    let cfg_path = repo.join(".balls/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg["plugins"] = serde_json::json!({
        "jira": {
            "enabled": true,
            "sync_on_change": false,
            "config_file": ".balls/plugins/jira.json",
            "participant": { "subscriptions": { event: { "policy": policy } } }
        }
    });
    std::fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
    git(repo, &["add", ".balls/config.json", ".balls/plugins"]);
    git(repo, &["commit", "-m", "configure jira", "--no-verify"]);
}

fn state_subject(repo: &Path) -> String {
    git(repo, &["log", "-1", "--format=%s", "balls/tasks"])
}

const REJECT: &str = r#"
    cat - >/dev/null
    printf '{"reject":{"reason":"CI is red on this branch"}}\n'
    exit 0
    "#;

#[test]
fn required_reject_aborts_update_and_rolls_back() {
    let bin = install_native_plugin("jira", REJECT);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "enforced");
    jira_policy(repo.path(), "update", "required");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--note", "poke"])
        .output()
        .unwrap();

    assert!(!out.status.success(), "required veto must abort the command");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("CI is red on this branch"),
        "reason must propagate verbatim: {stderr}"
    );
    // State branch rolled back: the appended note is gone with the
    // reverted update commit.
    let notes = read_task_notes(repo.path(), &id);
    assert!(
        !notes.iter().any(|n| n["text"] == "poke"),
        "the update commit must be rewound: {notes:?}"
    );
}

#[test]
fn best_effort_reject_ships_and_records_sync_status() {
    let bin = install_native_plugin("jira", REJECT);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "best-effort");
    jira_policy(repo.path(), "update", "best-effort");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--note", "poke"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "best-effort veto must not abort: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task = read_task_json(repo.path(), &id);
    let recorded = task["sync_status"]["jira"].as_str().unwrap_or_default();
    assert!(
        recorded.contains("CI is red on this branch"),
        "best-effort skip records the verbatim reason: {task}"
    );
}

#[test]
fn skip_override_ships_past_required_and_is_logged() {
    let bin = install_native_plugin("jira", REJECT);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "skipped");
    jira_policy(repo.path(), "update", "required");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--skip", "jira", "--note", "poke"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "--skip removes the required participant, so the event ships: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        state_subject(repo.path()).contains("[--skip=jira]"),
        "the §11 override must be logged in the state-branch commit: {}",
        state_subject(repo.path())
    );
    let notes = read_task_notes(repo.path(), &id);
    assert!(notes.iter().any(|n| n["text"] == "poke"), "update shipped");
}

#[test]
fn gating_reject_is_inert_and_ships() {
    // SPEC §9 gating: staging machinery is bl-a46d. Until then a
    // gating veto is inert at the command — the event ships, nothing
    // is recorded, nothing rolls back (distinct from required/best-
    // effort). Pins the `Staged => Inert` dispatch arm.
    let bin = install_native_plugin("jira", REJECT);
    let repo = new_repo();
    init_in(repo.path());
    let id = create_task(repo.path(), "gated");
    jira_policy(repo.path(), "update", "gating");
    create_auth(repo.path(), "jira");

    let out = bl(repo.path())
        .env("PATH", path_with(&[bin.path()]))
        .args(["update", &id, "--note", "poke"])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "a gating veto must not abort (bl-a46d staging is future work): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task = read_task_json(repo.path(), &id);
    assert!(
        task.get("sync_status").is_none(),
        "gating is not a best-effort skip — sync_status stays clean: {task}"
    );
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
