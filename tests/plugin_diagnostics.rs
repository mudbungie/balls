//! Plugin diagnostics channel: BALLS_DIAG_FD delivers NDJSON diagnostic
//! records that balls renders on stderr. Silent-and-safe when the
//! plugin ignores the channel entirely.

mod common;

use common::*;
use common::plugin::*;

fn run_sync(repo_path: &std::path::Path, bin_dir: &std::path::Path) -> std::process::Output {
    bl(repo_path)
        .env("PATH", path_with_mock(bin_dir))
        .arg("sync")
        .output()
        .unwrap()
}

#[test]
fn plugin_ignoring_diag_channel_is_silent() {
    // The stock mock plugin never inspects BALLS_DIAG_FD. Sync should
    // succeed and emit no diagnostic-shaped stderr lines — proof that
    // wiring up the channel is a no-op for unaware plugins.
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let out = run_sync(repo.path(), bin_dir.path());
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("plugin balls-plugin-mock sync"),
        "no diagnostic should be rendered: {stderr}"
    );
    assert!(
        !stderr.contains("invalid diagnostic JSON"),
        "no malformed-diag warning expected: {stderr}"
    );
}

#[test]
fn plugin_emits_single_diagnostic() {
    let snippet = r#"printf '%s\n' '{"level":"error","code":"E_REMOTE","message":"remote unreachable","hint":"check VPN","task_id":"bl-abcd"}' >&"$BALLS_DIAG_FD""#;
    let bin_dir = install_plugin_with_diag(snippet);
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let out = run_sync(repo.path(), bin_dir.path());
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("error [E_REMOTE]: remote unreachable"),
        "diag should be rendered: {stderr}"
    );
    assert!(stderr.contains("hint: check VPN"), "hint should render: {stderr}");
    assert!(stderr.contains("task: bl-abcd"), "task id should render: {stderr}");
}

#[test]
fn plugin_emits_multiple_diagnostics() {
    let snippet = r#"{
    printf '%s\n' '{"level":"warning","message":"first"}'
    printf '%s\n' '{"level":"info","message":"second"}'
    printf '%s\n' '{"level":"error","message":"third"}'
} >&"$BALLS_DIAG_FD""#;
    let bin_dir = install_plugin_with_diag(snippet);
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let out = run_sync(repo.path(), bin_dir.path());
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    for needle in ["warning : first", "info : second", "error : third"]
        .iter()
        .map(|n| n.replace(" : ", ": "))
    {
        assert!(stderr.contains(&needle), "missing {needle}: {stderr}");
    }
}

#[test]
fn plugin_emits_malformed_diagnostic_is_warned_not_fatal() {
    let snippet = r#"printf '%s\n' 'not json at all' >&"$BALLS_DIAG_FD""#;
    let bin_dir = install_plugin_with_diag(snippet);
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let out = run_sync(repo.path(), bin_dir.path());
    assert!(
        out.status.success(),
        "malformed diag must not fail the sync: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid diagnostic JSON"),
        "should warn about malformed diag: {stderr}"
    );
}

#[test]
fn plugin_mixed_valid_and_malformed_diagnostics() {
    // One malformed line should not block the valid ones around it.
    // A blank line is sprinkled in to exercise the empty-line skip
    // branch inside render_diagnostics.
    let snippet = r#"{
    printf '%s\n' '{"level":"error","message":"good one"}'
    printf '\n'
    printf '%s\n' 'garbage'
    printf '%s\n' '{"level":"warning","message":"after garbage"}'
} >&"$BALLS_DIAG_FD""#;
    let bin_dir = install_plugin_with_diag(snippet);
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let out = run_sync(repo.path(), bin_dir.path());
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error: good one"), "first valid: {stderr}");
    assert!(stderr.contains("warning: after garbage"), "second valid: {stderr}");
    assert!(stderr.contains("invalid diagnostic JSON"), "malformed warning: {stderr}");
}
