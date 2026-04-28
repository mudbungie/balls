//! Integration tests for `bl sync --review`, `--apply`, `--discard`,
//! `--list-staged`. Mirrors bl-a46d's test list:
//!
//! 1. `--review` stages a populated SyncReport without mutating state-
//!    branch HEAD.
//! 2. `--apply <id>` replays the staged report and commits it normally.
//! 3. `--discard <id>` cleans up without side effects.
//! 4. A participant's failure during apply propagates per its existing
//!    policy (warn-and-continue, same as live sync).

mod common;

use common::human_gate::*;
use common::plugin::*;
use common::*;
use std::fs;

#[test]
fn review_stages_report_without_mutating_state_branch() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    write_sync_response(repo.path(), &populated_sync_response("from review"));

    let head_before = state_head(repo.path());
    let pre_count = list_tasks_count(repo.path());

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["sync", "--review"])
        .assert()
        .success();

    let head_after = state_head(repo.path());
    assert_eq!(head_before, head_after, "state branch HEAD must not move");
    assert_eq!(
        list_tasks_count(repo.path()),
        pre_count,
        "no new task should land before apply"
    );

    let ids = collect_staged_ids(repo.path());
    assert_eq!(ids.len(), 1, "expected one staged sync report, got {ids:?}");

    let staged_path = pending_dir(repo.path()).join(format!("{}.json", ids[0]));
    let staged: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&staged_path).unwrap()).unwrap();
    assert_eq!(staged["plugin"], "mock");
    assert_eq!(staged["event"], "sync");
    assert_eq!(staged["report"]["created"][0]["title"], "from review");
}

#[test]
fn apply_replays_staged_report_and_commits() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    write_sync_response(repo.path(), &populated_sync_response("from apply"));

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["sync", "--review"])
        .assert()
        .success();

    let id = collect_staged_ids(repo.path()).pop().expect("staged id");
    let head_before = state_head(repo.path());

    bl(repo.path())
        .args(["sync", "--apply", &id])
        .assert()
        .success();

    let head_after = state_head(repo.path());
    assert_ne!(head_before, head_after, "apply should produce a new commit");
    assert!(
        collect_staged_ids(repo.path()).is_empty(),
        "staged file should be removed after apply"
    );

    let new_task_id = find_task_with_title(repo.path(), "from apply");
    let task = read_task_json(repo.path(), &new_task_id);
    assert_eq!(task["title"], "from apply");
    assert_eq!(task["external"]["mock"]["remote_key"], "MOCK-NEW");
}

#[test]
fn discard_drops_staged_report_without_applying() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    write_sync_response(
        repo.path(),
        &populated_sync_response("should never apply"),
    );

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["sync", "--review"])
        .assert()
        .success();

    let id = collect_staged_ids(repo.path()).pop().expect("staged id");
    let head_before = state_head(repo.path());
    let pre_count = list_tasks_count(repo.path());

    bl(repo.path())
        .args(["sync", "--discard", &id])
        .assert()
        .success();

    assert_eq!(state_head(repo.path()), head_before, "no commit on discard");
    assert_eq!(list_tasks_count(repo.path()), pre_count);
    assert!(
        collect_staged_ids(repo.path()).is_empty(),
        "discard should remove the staged file"
    );
    assert!(
        bl(repo.path())
            .args(["sync", "--apply", &id])
            .output()
            .unwrap()
            .status
            .code()
            .unwrap_or_default()
            != 0,
        "apply on a discarded id should fail"
    );
}

#[test]
fn apply_failure_for_unknown_task_propagates_per_existing_policy() {
    // Synthetic staged file referencing a task that does not exist.
    // apply_sync_report's per-item warn-and-continue policy is what
    // surfaces here — apply still succeeds and stderr names the
    // missing task. Same semantics live `bl sync` has.
    let repo = new_repo();
    init_in(repo.path());

    let dir = pending_dir(repo.path());
    fs::create_dir_all(&dir).unwrap();
    let staged = serde_json::json!({
        "event": "sync",
        "plugin": "mock",
        "staged_at": "2026-04-28T00:00:00Z",
        "report": {
            "created": [],
            "updated": [{
                "task_id": "bl-deadbeef",
                "fields": { "priority": 1 }
            }],
            "deleted": []
        }
    });
    let stage_id = "mock-fake";
    let path = dir.join(format!("{stage_id}.json"));
    fs::write(&path, serde_json::to_string_pretty(&staged).unwrap()).unwrap();

    let out = bl(repo.path())
        .args(["sync", "--apply", stage_id])
        .output()
        .unwrap();
    assert!(out.status.success(), "apply should succeed even with bad item");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("bl-deadbeef") || stderr.contains("unknown task"),
        "expected warning about unknown task: {stderr}"
    );
    assert!(
        !path.exists(),
        "apply should remove the staged file even on per-item warnings"
    );
}

#[test]
fn review_with_corrupt_config_warns_and_exits_clean() {
    let repo = new_repo();
    init_in(repo.path());
    fs::write(repo.path().join(".balls/config.json"), "not json").unwrap();
    let out = bl(repo.path())
        .args(["sync", "--review"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("plugin sync failed"),
        "expected plugin sync failure warning in --review: {stderr}"
    );
}

#[test]
fn review_warns_when_staging_directory_is_blocked() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    write_sync_response(repo.path(), &populated_sync_response("blocked"));

    fs::create_dir_all(repo.path().join(".balls/local")).unwrap();
    fs::write(repo.path().join(".balls/local/pending-sync"), b"x").unwrap();

    let out = bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["sync", "--review"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("warning: stage mock failed"),
        "expected stage failure warning: {stderr}"
    );
}

#[test]
fn list_staged_renders_pending_entries_and_empty_state() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let empty = bl(repo.path())
        .args(["sync", "--list-staged"])
        .output()
        .unwrap();
    assert!(empty.status.success());
    let stdout = String::from_utf8_lossy(&empty.stdout);
    assert!(stdout.contains("no staged"), "expected empty marker: {stdout}");

    write_sync_response(repo.path(), &populated_sync_response("listed"));
    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["sync", "--review"])
        .assert()
        .success();

    let listed = bl(repo.path())
        .args(["sync", "--list-staged"])
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed_stdout = String::from_utf8_lossy(&listed.stdout);
    assert!(
        listed_stdout.contains("mock") && listed_stdout.contains("created=1"),
        "expected list to show plugin + counts: {listed_stdout}"
    );
}
