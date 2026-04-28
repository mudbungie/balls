//! End-to-end coverage for the bl-8b71 native participant protocol.
//!
//! Each test spins up a real shell-script plugin that implements the
//! describe/propose subcommands (and `auth-check`/`auth-setup` so the
//! existing runner gating passes). The lifecycle is driven through
//! the real `bl` binary so the dispatcher routing, projection apply,
//! and SPEC §10 commit-policy planner all run.
//!
//! Three scenarios from the bl-8b71 task description:
//! 1. Native plugin contributes a remote view; merged Task carries
//!    both the canonical-side change (status) and the plugin-owned
//!    `external.<name>.*` slice.
//! 2. Native plugin reports a conflict on the first propose; the
//!    negotiation primitive retries (per SPEC §8) and converges on
//!    the second attempt.
//! 3. Mixed config: one legacy plugin, one native plugin, both
//!    subscribed to the same event. Both contribute, in stable
//!    subscription order, with independent failure policies.

mod common;

use common::native_plugin::{
    create_auth, install_legacy_plugin, install_native_plugin, path_with,
    write_plugin_config,
};
use common::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn native_propose_contributes_external_slice() {
    // Plugin owns external.jira.*; on every push event it returns a
    // proposed Task carrying { external: { jira: { remote_key } } }.
    // After `bl create`, the Task on disk should have the canonical
    // status (Open) AND the jira projection populated.
    let bin = install_native_plugin(
        "jira",
        r#"
        cat - >/dev/null
        printf '{"ok":{"task":{"external":{"jira":{"remote_key":"NATIVE-1","status":"todo"}}}}}\n'
        exit 0
        "#,
    );
    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");

    let id = run_create(&repo, &[bin.path()], "native ok path");
    let task = read_task_json(repo.path(), &id);
    assert_eq!(task["status"], "open");
    assert_eq!(
        task["external"]["jira"]["remote_key"], "NATIVE-1",
        "native plugin's external slice must land on the Task: {task}"
    );
    assert_eq!(task["external"]["jira"]["status"], "todo");
}

#[test]
fn native_conflict_then_ok_retries_and_converges() {
    // SPEC §8 retry semantics: the first propose returns a structured
    // conflict; the negotiation primitive's loop classifies it as
    // Conflict, calls fetch_remote_view (a no-op for native plugins
    // — the conflict's remote_view is informational; the plugin
    // tracks its own remote-state memory), and re-runs propose. The
    // plugin records its retry count under its auth dir so the
    // second invocation knows to return Ok. After resolution the
    // jira slice carries the AFTER-CONFLICT payload.
    let bin = install_native_plugin(
        "jira",
        r#"
        cat - >/dev/null
        COUNT_FILE="$STATE_DIR/propose-count"
        N=0
        if [ -f "$COUNT_FILE" ]; then N=$(cat "$COUNT_FILE"); fi
        N=$((N + 1))
        echo "$N" > "$COUNT_FILE"
        if [ "$N" = "1" ]; then
            printf '{"conflict":{"fields":["external.jira.remote_key"],"remote_view":{"external":{"jira":{"remote_key":"REMOTE-MOVED"}}},"hint":"remote moved"}}\n'
        else
            printf '{"ok":{"task":{"external":{"jira":{"remote_key":"AFTER-CONFLICT","attempts":'"$N"'}}}}}\n'
        fi
        exit 0
        "#,
    );
    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");

    let id = run_create(&repo, &[bin.path()], "conflict-then-ok");
    let task = read_task_json(repo.path(), &id);
    assert_eq!(
        task["external"]["jira"]["remote_key"], "AFTER-CONFLICT",
        "second-attempt ok payload must apply after conflict retry: {task}"
    );
    assert_eq!(
        task["external"]["jira"]["attempts"], 2,
        "plugin must have been re-invoked once after the conflict: {task}"
    );
}

#[test]
fn mixed_legacy_and_native_plugins_both_contribute() {
    // Config has two plugins: a legacy one (push-only) named "alpha"
    // and a native one named "beta" with describe + propose. After a
    // push event, both contribute to task.external. Plugin map iter
    // is sorted (BTreeMap), so "alpha" lands first and "beta" second
    // — verifying stable subscription order across protocols.
    // Independent failure policies: both BestEffort by default, so
    // an unrelated failure on one would not stop the other.
    let alpha = install_legacy_plugin("alpha");
    let beta = install_native_plugin(
        "beta",
        r#"
        cat - >/dev/null
        printf '{"ok":{"task":{"external":{"beta":{"native":true}}}}}\n'
        exit 0
        "#,
    );
    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["alpha", "beta"]);
    create_auth(repo.path(), "alpha");
    create_auth(repo.path(), "beta");

    let id = run_create(&repo, &[alpha.path(), beta.path()], "mixed config");
    let task = read_task_json(repo.path(), &id);
    assert_eq!(
        task["external"]["alpha"]["remote_key"],
        format!("LEGACY-{id}"),
        "legacy plugin must populate its external slice: {task}",
    );
    assert_eq!(
        task["external"]["beta"]["native"], true,
        "native plugin must populate its external slice: {task}",
    );
}

#[test]
fn native_propose_with_invalid_json_is_absorbed_as_skipped() {
    // Propose exits 0 but writes nothing parseable on stdout; the
    // runner returns Ok(None), the protocol classifies as Other, and
    // the BestEffort default failure policy absorbs the failure as
    // Skipped. The dispatcher records no contribution; the Task
    // still lands on disk with no jira slice.
    let bin = install_native_plugin(
        "jira",
        "cat - >/dev/null; echo 'this is not json'; exit 0",
    );
    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");

    let id = run_create(&repo, &[bin.path()], "garbage propose");
    let task = read_task_json(repo.path(), &id);
    assert!(
        task["external"].get("jira").is_none(),
        "no contribution expected when propose payload is unparseable: {task}"
    );
}

#[test]
fn native_plugin_skipped_when_event_not_in_subscriptions() {
    // describe declares only the "close" event. `bl create` triggers
    // claim, so the dispatcher must skip the plugin without invoking
    // propose. Verified by the absence of any external slice and by
    // the lack of a propose-count file the script writes when run.
    let bin_dir = tempfile::Builder::new().prefix("balls-skip-").tempdir().unwrap();
    let script = r#"#!/bin/sh
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
    auth-setup) mkdir -p "$AUTH_DIR" && echo '{"token":"t"}' > "$AUTH_DIR/token.json"; exit 0 ;;
    describe)
        echo '{"subscriptions":["close"],"projection":{"external_prefixes":["jira"]}}'
        exit 0
        ;;
    propose)
        cat - >/dev/null
        touch "$AUTH_DIR/propose-was-called"
        echo '{"ok":{"task":{"external":{"jira":{"k":"v"}}}}}'
        exit 0
        ;;
esac
"#;
    let path = bin_dir.path().join("balls-plugin-jira");
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();

    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");
    let id = run_create(&repo, &[bin_dir.path()], "skipped event");
    let task = read_task_json(repo.path(), &id);
    assert!(
        task["external"].get("jira").is_none(),
        "plugin must not contribute when event is not subscribed: {task}"
    );
    let propose_marker =
        repo.path().join(".balls/local/plugins/jira/propose-was-called");
    assert!(
        !propose_marker.exists(),
        "propose must not have been invoked for an unsubscribed event"
    );
}

#[test]
fn native_propose_owning_canonical_field_overlays_it() {
    // Plugin declares projection.owns = ["title"]; on propose it
    // returns a task with a different title. The applier must copy
    // that canonical field through (covers project_overlay's owns
    // loop, which the external-only tests don't exercise).
    let bin_dir = tempfile::Builder::new().prefix("balls-canonical-").tempdir().unwrap();
    let script = r#"#!/bin/sh
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
    auth-setup) mkdir -p "$AUTH_DIR" && echo '{"token":"t"}' > "$AUTH_DIR/token.json"; exit 0 ;;
    describe)
        echo '{"subscriptions":["claim","review","close","update"],"projection":{"owns":["title"],"external_prefixes":["jira"]}}'
        exit 0
        ;;
    propose)
        cat - >/dev/null
        echo '{"ok":{"task":{"title":"renamed-by-plugin","external":{"jira":{"k":"v"}}}}}'
        exit 0
        ;;
esac
"#;
    let path = bin_dir.path().join("balls-plugin-jira");
    std::fs::write(&path, script).unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).unwrap();

    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["jira"]);
    create_auth(repo.path(), "jira");
    let id = run_create(&repo, &[bin_dir.path()], "original title");
    let task = read_task_json(repo.path(), &id);
    assert_eq!(task["title"], "renamed-by-plugin", "owns overlay must apply: {task}");
    assert_eq!(task["external"]["jira"]["k"], "v");
}

fn run_create(repo: &Repo, bins: &[&std::path::Path], title: &str) -> String {
    let out = bl(repo.path())
        .env("PATH", path_with(bins))
        .args(["create", title])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "bl create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
