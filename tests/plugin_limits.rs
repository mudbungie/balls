//! Plugin runner resource-limit tests: a plugin that sleeps longer
//! than the timeout must be killed and reported, and a plugin that
//! emits more than the output cap must be rejected without OOMing
//! the parent.

mod common;

use common::plugin::*;
use common::*;
use std::path::Path;

fn path_with(bin: &Path) -> String {
    path_with_mock(bin)
}

#[test]
fn plugin_sync_timeout_is_killed_and_warned() {
    // BALLS_PLUGIN_TIMEOUT_SECS=2 + sleep 20 means the test budget
    // is ~3s: the plugin is killed at 2s, bl sync wraps up
    // immediately after.
    let bin = install_plugin_script(
        "        sleep 20
        exit 0",
    );
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let start = std::time::Instant::now();
    let out = bl(repo.path())
        .env("PATH", path_with(bin.path()))
        .env("BALLS_PLUGIN_TIMEOUT_SECS", "2")
        .arg("sync")
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(out.status.success(), "bl sync should absorb timeout");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("timed out"),
        "expected timeout warning, got: {stderr}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "timeout should fire near 2s; took {elapsed:?}"
    );
}

#[test]
fn plugin_sync_huge_stdout_is_bounded_and_warned() {
    // BALLS_PLUGIN_MAX_STREAM_BYTES=1024 + a 128KB emission forces
    // the drain loop to iterate past the cap more than once — some
    // reads come in when the buffer is already full, exercising
    // both the "take < n" and the "buf.len() >= cap" branches.
    let bin = install_plugin_script(
        "        head -c 131072 /dev/urandom | base64
        exit 0",
    );
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let out = bl(repo.path())
        .env("PATH", path_with(bin.path()))
        .env("BALLS_PLUGIN_MAX_STREAM_BYTES", "1024")
        .arg("sync")
        .output()
        .unwrap();

    assert!(out.status.success(), "bl sync should absorb overflow");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("exceeded"),
        "expected overflow warning, got: {stderr}"
    );
}

#[test]
fn plugin_push_timeout_is_killed_and_warned() {
    // Same as sync timeout, but via bl create which triggers
    // plugin push.
    let bin = install_plugin_script(
        "        sleep 20
        exit 0",
    );
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let start = std::time::Instant::now();
    let out = bl(repo.path())
        .env("PATH", path_with(bin.path()))
        .env("BALLS_PLUGIN_TIMEOUT_SECS", "2")
        .args(["create", "timeout me"])
        .output()
        .unwrap();
    let elapsed = start.elapsed();

    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("timed out"), "stderr: {stderr}");
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "took {elapsed:?}"
    );
}
