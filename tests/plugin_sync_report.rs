//! Plugin sync report processing: create, update, delete, defaults, edge cases.

mod common;

use common::*;
use common::plugin::*;

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
    assert!(
        t["synced_at"]["mock"].is_string(),
        "sync-create should populate synced_at.mock: {}",
        t["synced_at"]
    );
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
            "task_id": "{id}",
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
    }}"#));

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
    let synced = task["synced_at"]["mock"].as_str().unwrap_or("");
    assert!(
        !synced.is_empty(),
        "sync-update should bump synced_at.mock: {}",
        task["synced_at"]
    );

    let notes = read_task_notes(repo.path(), &id);
    assert!(
        notes.iter().any(|n| n["text"].as_str().unwrap().contains("Priority changed by PM")),
        "should have a note from the sync: {notes:?}"
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
            "task_id": "{id}",
            "reason": "Issue PROJ-789 deleted in Jira"
        }}]
    }}"#));

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    let task = read_task_json(repo.path(), &id);
    assert_eq!(task["status"].as_str().unwrap(), "deferred", "deleted task should be deferred");
    assert!(
        task["synced_at"]["mock"].is_string(),
        "sync-defer should bump synced_at.mock: {}",
        task["synced_at"]
    );
    let notes = read_task_notes(repo.path(), &id);
    assert!(
        notes.iter().any(|n| n["text"].as_str().unwrap().contains("PROJ-789 deleted")),
        "should have a deletion note: {notes:?}"
    );
}

#[test]
fn sync_created_uses_defaults_for_missing_fields() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

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

    let id = {
        let out = bl(repo.path())
            .env("PATH", path_with_mock(bin_dir.path()))
            .args(["create", "will be closed"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

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

    write_sync_response(repo.path(), &format!(r#"{{
        "deleted": [{{ "task_id": "{id}", "reason": "should not affect closed task" }}]
    }}"#));

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    let task_path = repo.path().join(format!(".balls/tasks/{id}.json"));
    if task_path.exists() {
        let task = read_task_json(repo.path(), &id);
        assert_ne!(
            task["status"].as_str().unwrap(),
            "deferred",
            "closed/archived task should not be deferred"
        );
    }
}
