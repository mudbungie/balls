//! Plugin push tests: create/update trigger push, auth gating, failure tolerance.

mod common;

use common::*;
use common::plugin::*;
use std::fs;

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
        ext.is_null() || ext.as_object().is_none_or(|m| m.is_empty()),
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
