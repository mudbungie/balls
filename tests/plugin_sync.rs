//! Plugin sync tests: triggers, auth gating, failure tolerance, filtering.

mod common;

use common::*;
use common::plugin::*;
use std::fs;

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
fn auth_check_and_sync_receive_config_path() {
    // Regression for plugin authors that target multiple instances: every
    // subcommand — including auth-check and auth-setup — must be given the
    // --config path so the plugin knows which instance it's acting on.
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
    let config_suffix = ".balls/plugins/mock.json";
    let auth_check_line = log_contents
        .lines()
        .find(|l| l.contains("auth-check"))
        .unwrap_or_else(|| panic!("expected auth-check in log: {log_contents}"));
    assert!(
        auth_check_line.contains(config_suffix),
        "auth-check should receive --config: {auth_check_line}"
    );
    let sync_line = log_contents
        .lines()
        .find(|l| l.contains(" sync "))
        .unwrap_or_else(|| panic!("expected sync in log: {log_contents}"));
    assert!(
        sync_line.contains(config_suffix),
        "sync should receive --config: {sync_line}"
    );
}

#[test]
fn story_71_plugin_unavailable_does_not_block_sync() {
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
fn sync_warns_when_plugin_framework_errors() {
    // Corrupt the balls config so `run_plugin_sync` errors before
    // touching any plugin — exercises the outer Err match arm in
    // cmd_sync, which should warn but not fail the command.
    let repo = new_repo();
    init_in(repo.path());
    std::fs::write(repo.path().join(".balls/config.json"), "not json").unwrap();
    let out = bl(repo.path()).arg("sync").output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("plugin sync failed"),
        "expected plugin sync failure warning: {stderr}"
    );
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
        "should warn about auth: {stderr}"
    );
    assert!(
        stderr.contains("--config") && stderr.contains(".balls/plugins/mock.json"),
        "re-auth hint should reference the plugin config path: {stderr}"
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
            "task_id": "{id}",
            "fields": {{ "priority": 1 }}
        }}],
        "deleted": []
    }}"#));

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

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

    let sync_stdin_path = format!("{}.sync-stdin", log.display());
    let stdin_content = fs::read_to_string(&sync_stdin_path).unwrap_or_default();
    assert!(
        stdin_content.contains(&id1) && stdin_content.contains(&id2),
        "sync stdin should contain all task IDs: {stdin_content}"
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

    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        log_contents.contains(&format!("task={id}")),
        "plugin should receive --task flag: {log_contents}"
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
        "plugin should receive remote key as --task: {log_contents}"
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

    bl(repo.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();

    let task = read_task_json(repo.path(), &id);
    assert_eq!(task["priority"].as_u64().unwrap(), 3, "task unchanged after empty sync");
    assert_eq!(task["status"].as_str().unwrap(), "open");
}
