//! Plugin runner edge-case coverage: empty responses, invalid JSON,
//! PATH-missing, and auth-check failures via spawn error.

mod common;

use common::plugin::*;
use common::*;
use std::fs;
use std::path::Path;

fn remote_path_with(bin: &Path) -> String {
    path_with_mock(bin)
}

#[test]
fn plugin_sync_empty_stdout_is_silently_skipped() {
    let bin_dir = install_plugin_with_body("");
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    bl(repo.path())
        .env("PATH", remote_path_with(bin_dir.path()))
        .arg("sync")
        .assert()
        .success();
}

#[test]
fn plugin_sync_invalid_json_warns() {
    let bin_dir = install_plugin_with_body("{ not json");
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    let out = bl(repo.path())
        .env("PATH", remote_path_with(bin_dir.path()))
        .arg("sync")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid JSON"), "stderr: {}", stderr);
}

#[test]
fn plugin_push_empty_stdout_is_silently_skipped() {
    let bin_dir = install_plugin_with_body("");
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    // `bl create` triggers plugin push for the new task. Empty stdout
    // must not fail the command.
    bl(repo.path())
        .env("PATH", remote_path_with(bin_dir.path()))
        .args(["create", "empty-push task"])
        .assert()
        .success();
}

#[test]
fn plugin_push_invalid_json_warns() {
    let bin_dir = install_plugin_with_body("not-json");
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    let out = bl(repo.path())
        .env("PATH", remote_path_with(bin_dir.path()))
        .args(["create", "bad push task"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("invalid JSON"), "stderr: {}", stderr);
}

#[test]
fn plugin_push_not_on_path_is_silently_skipped() {
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    // Narrow PATH so the plugin is not resolvable. `bl create` must
    // succeed anyway — push is best-effort.
    bl(repo.path())
        .env("PATH", "/usr/bin:/bin")
        .args(["create", "no-plugin task"])
        .assert()
        .success();
}

#[test]
fn plugin_push_fails_with_nonzero_exit_warns() {
    let (bin_dir, _log) = install_failing_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    let out = bl(repo.path())
        .env("PATH", remote_path_with(bin_dir.path()))
        .args(["create", "failing push task"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("push failed"), "stderr: {}", stderr);
}

#[test]
fn plugin_auth_check_spawn_error_returns_false() {
    // When the plugin executable is present but unexecutable, the
    // spawn call inside auth_check returns an io::Error — exercises
    // the `Err(_) => false` branch in runner.rs.
    let bin_dir = tempfile::tempdir().unwrap();
    let path = bin_dir.path().join("balls-plugin-mock");
    fs::write(&path, "#!/bin/false\n").unwrap();
    // Not executable.
    let mut p = fs::metadata(&path).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    p.set_mode(0o644);
    fs::set_permissions(&path, p).unwrap();

    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());
    // With a non-executable mock, `which` still finds the file but
    // Command spawn refuses. Plugin falls through gracefully.
    bl(repo.path())
        .env("PATH", format!("{}:{}", bin_dir.path().display(), "/usr/bin:/bin"))
        .arg("sync")
        .assert()
        .success();
}

