//! apply_sync_report edge-case coverage: title update, unknown fields,
//! missing tasks, closed-already deletes, default-reason deletes, and
//! a forced create failure via a read-only state worktree.

mod common;

use common::plugin::*;
use common::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn with_path(bin: &std::path::Path) -> String {
    path_with_mock(bin)
}

#[test]
fn title_update_unknown_field_and_missing_task() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = create_task(repo.path(), "original title");
    write_sync_response(
        repo.path(),
        &format!(
            r#"{{
              "created": [],
              "updated": [
                {{"task_id": "{id}",
                  "fields": {{"title": "new title", "never_heard_of": 42}},
                  "external": {{}}, "add_note": null}},
                {{"task_id": "bl-ffff",
                  "fields": {{}}, "external": {{}}, "add_note": null}}
              ],
              "deleted": []
            }}"#,
            id = id
        ),
    );

    let out = bl(repo.path())
        .env("PATH", with_path(bin_dir.path()))
        .arg("sync")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown task bl-ffff"),
        "expected missing-task warning: {}",
        stderr
    );
    let task = read_task_json(repo.path(), &id);
    assert_eq!(task["title"].as_str().unwrap(), "new title");
}

#[test]
fn deleted_already_closed_is_skipped() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    // Patch a task to status=closed without archiving, then feed a
    // deleted report — the apply_deleted handler must short-circuit
    // on the Closed status.
    let id = create_task(repo.path(), "closed-not-archived");
    let p = repo.path().join(".balls/tasks").join(format!("{}.json", id));
    let mut j: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
    j["status"] = "closed".into();
    fs::write(&p, serde_json::to_string_pretty(&j).unwrap()).unwrap();

    write_sync_response(
        repo.path(),
        &format!(
            r#"{{"created":[],"updated":[],"deleted":[
              {{"task_id": "{id}", "reason": "gone"}}
            ]}}"#,
            id = id
        ),
    );
    bl(repo.path())
        .env("PATH", with_path(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();
}

#[test]
fn create_failure_is_warned_not_fatal() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let tasks_dir = repo.path().join(".balls/worktree/.balls/tasks");
    let saved = fs::metadata(&tasks_dir).unwrap().permissions();
    let mut ro = saved.clone();
    ro.set_mode(0o555);
    fs::set_permissions(&tasks_dir, ro).unwrap();

    write_sync_response(
        repo.path(),
        r#"{"created":[
              {"title":"from plugin","task_type":"task","priority":3,
               "status":"open","description":"","tags":[],"external":{}}
            ],"updated":[],"deleted":[]}"#,
    );
    let out = bl(repo.path())
        .env("PATH", with_path(bin_dir.path()))
        .arg("sync")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    fs::set_permissions(&tasks_dir, saved).unwrap();

    assert!(out.status.success());
    assert!(
        stderr.contains("sync-report create failed"),
        "expected create warning: {}",
        stderr
    );
}

#[test]
fn deleted_with_empty_reason_uses_default() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = create_task(repo.path(), "to defer with default reason");
    write_sync_response(
        repo.path(),
        &format!(
            r#"{{"created":[],"updated":[],"deleted":[
              {{"task_id": "{id}", "reason": ""}}
            ]}}"#,
            id = id
        ),
    );
    bl(repo.path())
        .env("PATH", with_path(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();
    let notes = read_task_notes(repo.path(), &id);
    assert!(
        notes
            .iter()
            .any(|n| n["text"].as_str().unwrap().contains("Deleted in remote tracker")),
        "expected default-reason note, got: {:?}",
        notes
    );
}
