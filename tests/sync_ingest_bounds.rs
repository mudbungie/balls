//! Generous anti-DoS backstops on plugin sync ingest (bl-4673).
//!
//! Pathological inputs — a create flood, an oversized single field, a
//! raised stream cap fronting a runaway plugin — must degrade
//! gracefully (truncate/skip + diagnostic, sync still applies) and
//! never OOM or reject the whole sync.

mod common;

use common::plugin::*;
use common::*;

fn with_path(bin: &std::path::Path) -> String {
    path_with_mock(bin)
}

#[test]
fn create_flood_is_clamped_and_warned_not_failed() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    // Five creates, ceiling of three: the first three land, the
    // overflow is skipped with a warning, the sync still succeeds.
    let entries: Vec<String> = (0..5)
        .map(|i| format!(r#"{{"title":"flood-{i}"}}"#))
        .collect();
    write_sync_response(
        repo.path(),
        &format!(
            r#"{{"created":[{}],"updated":[],"deleted":[]}}"#,
            entries.join(",")
        ),
    );

    let out = bl(repo.path())
        .env("PATH", with_path(bin_dir.path()))
        .env("BALLS_PLUGIN_MAX_SYNC_CREATES", "3")
        .arg("sync")
        .output()
        .unwrap();

    assert!(out.status.success(), "flood must not fail the sync");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("2 create(s) over the 3 flood backstop"),
        "expected flood warning, got: {stderr}"
    );

    let list = bl(repo.path())
        .args(["list", "--json", "--all"])
        .output()
        .unwrap();
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8_lossy(&list.stdout)).unwrap();
    let flooded = tasks
        .iter()
        .filter(|t| {
            t["title"]
                .as_str()
                .is_some_and(|s| s.starts_with("flood-"))
        })
        .count();
    assert_eq!(flooded, 3, "exactly the ceiling should be created");
}

#[test]
fn oversized_field_is_truncated_not_rejected() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let big_title = "Z".repeat(5000);
    let big_desc = "Q".repeat(5000);
    write_sync_response(
        repo.path(),
        &format!(
            r#"{{"created":[{{"title":"{big_title}","description":"{big_desc}"}}],
                "updated":[],"deleted":[]}}"#
        ),
    );

    let out = bl(repo.path())
        .env("PATH", with_path(bin_dir.path()))
        .env("BALLS_PLUGIN_MAX_SYNC_FIELD_BYTES", "64")
        .arg("sync")
        .output()
        .unwrap();

    assert!(out.status.success(), "oversize field must not fail sync");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("title field was 5000 bytes"),
        "expected title truncation warning, got: {stderr}"
    );
    assert!(
        stderr.contains("description field was 5000 bytes"),
        "expected description truncation warning, got: {stderr}"
    );

    let list = bl(repo.path())
        .args(["list", "--json", "--all"])
        .output()
        .unwrap();
    let tasks: Vec<serde_json::Value> =
        serde_json::from_str(&String::from_utf8_lossy(&list.stdout)).unwrap();
    let created = tasks
        .iter()
        .find(|t| t["title"].as_str().is_some_and(|s| s.starts_with("ZZZ")))
        .expect("the task is created despite the oversize fields");

    let title = created["title"].as_str().unwrap();
    let desc = created["description"].as_str().unwrap();
    assert!(title.len() < 5000, "title should be clipped: {}", title.len());
    assert!(
        title.contains("balls truncated this field"),
        "title carries a visible marker: {title}"
    );
    assert!(
        desc.contains("balls truncated this field"),
        "description carries a visible marker"
    );
}

#[test]
fn raised_stream_cap_cannot_disable_absolute_backstop() {
    // A raised per-stream cap (100 MB) would, on its own, wave a
    // 128 KB runaway through. The 1 KiB absolute backstop still
    // bounds the buffer and discards the report — proving "I needed a
    // bigger sync" can never become "a plugin may stream me to death".
    let bin = install_plugin_script(
        "        head -c 131072 /dev/urandom | base64
        exit 0",
    );
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let out = bl(repo.path())
        .env("PATH", with_path(bin.path()))
        .env("BALLS_PLUGIN_MAX_STREAM_BYTES", "100000000")
        .env("BALLS_PLUGIN_ABS_MAX_STREAM_BYTES", "1024")
        .arg("sync")
        .output()
        .unwrap();

    assert!(out.status.success(), "bl sync should absorb the overflow");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("exceeded 1024 bytes"),
        "backstop (1024), not the raised cap, must bind: {stderr}"
    );
    assert!(
        stderr.contains("BALLS_PLUGIN_ABS_MAX_STREAM_BYTES"),
        "discard warning should name the override knob: {stderr}"
    );
}
