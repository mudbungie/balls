//! Phase 5: plugin system. We use a shell script mock plugin that records
//! invocations and responds with canned output.
//!
//! Covers user stories 64–72.

mod common;

use common::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

// ---------------------------------------------------------------------------
// Dummy plugin: full-protocol mock that exercises auth, push, and sync.
// ---------------------------------------------------------------------------

/// Install a full-protocol `ball-plugin-mock` script into a temp bin dir.
/// Returns (bin_dir, log_path).
///
/// The plugin implements:
///   auth-check  — exit 0 if $AUTH_DIR/token.json exists, else exit 1
///   auth-setup  — creates $AUTH_DIR/token.json
///   push        — reads task JSON from stdin, writes PushResponse to stdout
///   sync        — reads tasks from stdin, returns canned SyncReport or empty
fn install_mock_plugin() -> (tempfile::TempDir, std::path::PathBuf) {
    let bin_dir = tempfile::Builder::new()
        .prefix("ball-mock-bin-")
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
    let script_path = bin_dir.path().join("ball-plugin-mock");
    fs::write(&script_path, script).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();
    (bin_dir, log_file)
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

    // Commit config changes so worktrees created later include them.
    git(repo_path, &["add", ".ball/config.json", ".ball/plugins/mock.json"]);
    git(repo_path, &["commit", "-m", "configure mock plugin", "--no-verify"]);
}

/// Create the mock auth token so auth-check passes.
fn create_mock_auth(repo_path: &std::path::Path) {
    let auth_dir = repo_path.join(".ball/local/plugins/mock");
    fs::create_dir_all(&auth_dir).unwrap();
    fs::write(auth_dir.join("token.json"), r#"{"token":"mock-token"}"#).unwrap();
}

fn path_with_mock(bin_dir: &std::path::Path) -> String {
    format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

// ---------------------------------------------------------------------------
// Failing plugin (for failure-tolerance tests)
// ---------------------------------------------------------------------------

/// Install a mock plugin that passes auth-check but fails on push/sync.
fn install_failing_plugin() -> (tempfile::TempDir, std::path::PathBuf) {
    let bin_dir = tempfile::Builder::new()
        .prefix("ball-failing-bin-")
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
    let path = bin_dir.path().join("ball-plugin-mock");
    fs::write(&path, script).unwrap();
    let mut p = fs::metadata(&path).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(&path, p).unwrap();
    (bin_dir, log_file)
}

// ===========================================================================
// Tests: existing behavior (backward compat)
// ===========================================================================

#[test]
fn story_65_create_triggers_plugin_push() {
    let (bin_dir, log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
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
    create_mock_auth(repo.path());

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
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

#[test]
fn plugin_push_failure_is_warned_not_fatal() {
    let (bin_dir, log) = install_failing_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
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
    create_mock_auth(repo.path());

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();
    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    assert!(log_contents.contains("sync"));
}

// ===========================================================================
// Tests: push write-back (new behavior)
// ===========================================================================

#[test]
fn push_populates_external_field() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = {
        let out = bl(repo.path())
            .env("PATH", path_with_mock(bin_dir.path()))
            .args(["create", "external field test"])
            .output()
            .unwrap();
        assert!(out.status.success(), "create failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let task = read_task_json(repo.path(), &id);
    let ext = &task["external"]["mock"];
    assert_eq!(
        ext["remote_key"].as_str().unwrap(),
        format!("MOCK-{}", id),
        "push should populate external.mock.remote_key, task.external={}", task["external"]
    );
    assert_eq!(
        ext["remote_url"].as_str().unwrap(),
        format!("https://mock.example/MOCK-{}", id),
    );
    assert!(ext["synced_at"].is_string(), "synced_at should be present");
}

#[test]
fn push_skipped_without_auth() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    // Deliberately NOT calling create_mock_auth — auth-check should fail

    let id = {
        let out = bl(repo.path())
            .env("PATH", path_with_mock(bin_dir.path()))
            .args(["create", "no auth"])
            .output()
            .unwrap();
        assert!(out.status.success(), "create should succeed without auth");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let task = read_task_json(repo.path(), &id);
    let ext = &task["external"];
    assert!(
        ext.is_null() || ext.as_object().map_or(true, |m| m.is_empty()),
        "external should be empty when auth fails: {:?}",
        ext
    );
}

#[test]
fn update_writes_back_push_response() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = {
        let out = bl(repo.path())
            .env("PATH", path_with_mock(bin_dir.path()))
            .args(["create", "update writeback"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // Update the task — push should fire again and write back
    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["update", &id, "priority=1"])
        .assert()
        .success();

    let task = read_task_json(repo.path(), &id);
    assert_eq!(
        task["external"]["mock"]["remote_key"].as_str().unwrap(),
        format!("MOCK-{}", id),
        "update should write back push response to external"
    );
}

// ===========================================================================
// Tests: auth-check gating
// ===========================================================================

#[test]
fn story_70_auth_expired_warns_and_skips() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    // No auth token — auth-check returns 1

    let out = bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .output()
        .unwrap();
    assert!(out.status.success(), "sync should succeed even with expired auth");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("auth") || stderr.contains("expired") || stderr.contains("auth-setup"),
        "should warn about auth: {}",
        stderr
    );
}

// ===========================================================================
// Tests: sync report processing
// ===========================================================================

fn write_sync_response(repo_path: &std::path::Path, response: &str) {
    let response_path = repo_path.join(".ball/plugins/mock.json.sync-response");
    fs::write(response_path, response).unwrap();
}

#[test]
fn sync_creates_task_from_remote() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    write_sync_response(repo.path(), r#"{
        "created": [{
            "title": "Remote issue from Jira",
            "type": "bug",
            "priority": 1,
            "status": "open",
            "description": "This was created in Jira",
            "tags": ["imported", "jira"],
            "external": {
                "remote_key": "PROJ-999",
                "synced_at": "2026-04-10T00:00:00Z"
            }
        }],
        "updated": [],
        "deleted": []
    }"#);

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    // Find the newly created task
    let out = bl(repo.path())
        .args(["list", "--json", "--all"])
        .output()
        .unwrap();
    let list_stdout = String::from_utf8_lossy(&out.stdout);
    let tasks: Vec<serde_json::Value> = serde_json::from_str(&list_stdout).unwrap();

    let remote_task = tasks
        .iter()
        .find(|t| t["title"].as_str() == Some("Remote issue from Jira"));
    assert!(
        remote_task.is_some(),
        "sync should create a local task from the remote: {:?}",
        tasks.iter().map(|t| t["title"].as_str()).collect::<Vec<_>>()
    );

    let t = remote_task.unwrap();
    assert_eq!(t["type"].as_str().unwrap(), "bug");
    assert_eq!(t["priority"].as_u64().unwrap(), 1);
    assert_eq!(t["description"].as_str().unwrap(), "This was created in Jira");
    assert_eq!(t["external"]["mock"]["remote_key"].as_str().unwrap(), "PROJ-999");
}

#[test]
fn sync_updates_existing_task() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = create_task(repo.path(), "task to update via sync");

    write_sync_response(repo.path(), &format!(r#"{{
        "created": [],
        "updated": [{{
            "task_id": "{}",
            "fields": {{
                "priority": 1,
                "status": "in_progress",
                "description": "Updated from remote"
            }},
            "external": {{
                "remote_key": "PROJ-100",
                "synced_at": "2026-04-10T00:00:00Z"
            }},
            "add_note": "Priority changed by PM in Jira"
        }}],
        "deleted": []
    }}"#, id));

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    let task = read_task_json(repo.path(), &id);
    assert_eq!(task["priority"].as_u64().unwrap(), 1, "priority should be updated");
    assert_eq!(task["status"].as_str().unwrap(), "in_progress", "status should be updated");
    assert_eq!(task["description"].as_str().unwrap(), "Updated from remote");
    assert_eq!(task["external"]["mock"]["remote_key"].as_str().unwrap(), "PROJ-100");

    // Check that the note was appended
    let notes = task["notes"].as_array().unwrap();
    assert!(
        notes.iter().any(|n| n["text"].as_str().unwrap().contains("Priority changed by PM")),
        "should have a note from the sync: {:?}",
        notes
    );
}

#[test]
fn sync_defers_deleted_task() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = create_task(repo.path(), "task to delete via sync");

    write_sync_response(repo.path(), &format!(r#"{{
        "created": [],
        "updated": [],
        "deleted": [{{
            "task_id": "{}",
            "reason": "Issue PROJ-789 deleted in Jira"
        }}]
    }}"#, id));

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    let task = read_task_json(repo.path(), &id);
    assert_eq!(task["status"].as_str().unwrap(), "deferred", "deleted task should be deferred");
    let notes = task["notes"].as_array().unwrap();
    assert!(
        notes.iter().any(|n| n["text"].as_str().unwrap().contains("PROJ-789 deleted")),
        "should have a deletion note: {:?}",
        notes
    );
}

#[test]
fn sync_with_auth_expired_skips_plugin() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    // No auth token

    let id = create_task(repo.path(), "should not be affected");

    write_sync_response(repo.path(), &format!(r#"{{
        "created": [],
        "updated": [{{
            "task_id": "{}",
            "fields": {{ "priority": 1 }}
        }}],
        "deleted": []
    }}"#, id));

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    // Task should NOT be updated since auth failed
    let task = read_task_json(repo.path(), &id);
    assert_eq!(
        task["priority"].as_u64().unwrap(),
        3,
        "task should not be modified when auth is expired"
    );
}

#[test]
fn sync_sends_all_tasks_on_stdin() {
    let (bin_dir, log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id1 = create_task(repo.path(), "first task");
    let id2 = create_task(repo.path(), "second task");

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    // The plugin should have received all tasks on stdin
    let sync_stdin_path = format!("{}.sync-stdin", log.display());
    let stdin_content = fs::read_to_string(&sync_stdin_path).unwrap_or_default();
    assert!(
        stdin_content.contains(&id1) && stdin_content.contains(&id2),
        "sync stdin should contain all task IDs: {}",
        stdin_content
    );
}

#[test]
fn sync_single_task_by_local_id() {
    let (bin_dir, log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = create_task(repo.path(), "filtered sync target");

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["sync", "--task", &id])
        .assert()
        .success();

    // Verify the plugin received the --task flag
    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        log_contents.contains(&format!("task={}", id)),
        "plugin should receive --task flag: {}",
        log_contents
    );
}

#[test]
fn sync_single_task_by_remote_key() {
    let (bin_dir, log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["sync", "--task", "PROJ-123"])
        .assert()
        .success();

    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        log_contents.contains("task=PROJ-123"),
        "plugin should receive remote key as --task: {}",
        log_contents
    );
}

#[test]
fn sync_empty_report_is_noop() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = create_task(repo.path(), "should be unchanged");

    // Default sync response is empty — no canned file
    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    let task = read_task_json(repo.path(), &id);
    assert_eq!(task["priority"].as_u64().unwrap(), 3, "task unchanged after empty sync");
    assert_eq!(task["status"].as_str().unwrap(), "open");
}

#[test]
fn sync_created_uses_defaults_for_missing_fields() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    // Minimal created entry — only title provided
    write_sync_response(repo.path(), r#"{
        "created": [{
            "title": "Minimal remote issue",
            "external": {
                "remote_key": "PROJ-MIN-1"
            }
        }]
    }"#);

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    let out = bl(repo.path())
        .args(["list", "--json", "--all"])
        .output()
        .unwrap();
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
    let t = tasks
        .iter()
        .find(|t| t["title"].as_str() == Some("Minimal remote issue"))
        .expect("should find the created task");

    assert_eq!(t["type"].as_str().unwrap(), "task", "default type");
    assert_eq!(t["priority"].as_u64().unwrap(), 3, "default priority");
    assert_eq!(t["status"].as_str().unwrap(), "open", "default status");
}

#[test]
fn sync_deleted_skips_already_closed() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    // Create a task and close it manually
    let id = {
        let out = bl(repo.path())
            .env("PATH", path_with_mock(bin_dir.path()))
            .args(["create", "will be closed"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // Claim and close
    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["claim", &id])
        .assert()
        .success();
    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["close", &id])
        .assert()
        .success();

    // Now try to defer it via sync — should be skipped (task is archived/closed)
    write_sync_response(repo.path(), &format!(r#"{{
        "deleted": [{{ "task_id": "{}", "reason": "should not affect closed task" }}]
    }}"#, id));

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    // Task was archived by close — it should not exist as a file anymore,
    // or if it does, it should still be closed, not deferred
    let task_path = repo.path().join(format!(".ball/tasks/{}.json", id));
    if task_path.exists() {
        let task = read_task_json(repo.path(), &id);
        assert_ne!(
            task["status"].as_str().unwrap(),
            "deferred",
            "closed/archived task should not be deferred"
        );
    }
    // If the file doesn't exist (archived), that's also correct
}
