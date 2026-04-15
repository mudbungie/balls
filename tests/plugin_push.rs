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
        "expected plugin push call in log: {log_contents}"
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
        format!("MOCK-{id}"),
        "push should populate external.mock.remote_key, task.external={}", task["external"]
    );
    assert_eq!(
        ext["remote_url"].as_str().unwrap(),
        format!("https://mock.example/MOCK-{id}"),
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
        ext.is_null() || ext.as_object().is_none_or(serde_json::Map::is_empty),
        "external should be empty when auth fails: {ext:?}"
    );
}

#[test]
fn push_records_top_level_synced_at_for_plugin() {
    // After a successful push response, balls writes
    // task.synced_at[plugin_name] so plugins can use it for
    // conflict resolution on subsequent syncs without maintaining
    // their own side-cache.
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = {
        let out = bl(repo.path())
            .env("PATH", path_with_mock(bin_dir.path()))
            .args(["create", "synced_at writeback"])
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let task = read_task_json(repo.path(), &id);
    let ts = task["synced_at"]["mock"].as_str().unwrap_or("");
    assert!(
        !ts.is_empty(),
        "synced_at.mock should be set after push: {}",
        task["synced_at"]
    );
    // Should be a parseable RFC3339 timestamp.
    chrono::DateTime::parse_from_rfc3339(ts).expect("synced_at must be RFC3339");
}

#[test]
fn subsequent_push_includes_synced_at_in_stdin() {
    // On the second push (from `bl update`), the task JSON sent on
    // stdin should carry the synced_at field that the first push
    // wrote, proving plugins see what balls has recorded.
    let (bin_dir, log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = {
        let out = bl(repo.path())
            .env("PATH", path_with_mock(bin_dir.path()))
            .args(["create", "second push sees synced_at"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // Truncate the stdin log so the next assertion inspects only
    // the update-triggered push, not the original create push
    // (which had no synced_at yet).
    let stdin_path = format!("{}.stdin", log.display());
    fs::write(&stdin_path, "").unwrap();

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["update", &id, "priority=1"])
        .assert()
        .success();

    let stdin = fs::read_to_string(&stdin_path).unwrap_or_default();
    assert!(
        stdin.contains("\"synced_at\""),
        "second push stdin should include synced_at: {stdin}"
    );
    assert!(
        stdin.contains("\"mock\""),
        "second push stdin should name the mock plugin inside synced_at: {stdin}"
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
        format!("MOCK-{id}"),
        "update should write back push response to external"
    );
}
