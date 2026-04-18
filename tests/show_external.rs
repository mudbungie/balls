//! `bl show` surfaces plugin-provided remote locations (ticket id, URL)
//! so agents don't have to dig through `--json`. Convention per README:
//! plugins populate `task.external.<plugin>.{remote_key,remote_url}`.

mod common;

use common::*;
use common::plugin::*;
use std::fs;

fn rewrite_external(repo_path: &std::path::Path, id: &str, external: serde_json::Value) {
    let task_path = repo_path.join(".balls/tasks").join(format!("{id}.json"));
    let mut task: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&task_path).unwrap()).unwrap();
    task["external"] = external;
    fs::write(&task_path, serde_json::to_string_pretty(&task).unwrap()).unwrap();
}

#[test]
fn show_renders_external_remote_locations() {
    let (bin_dir, _log) = install_mock_plugin();
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());
    create_mock_auth(repo.path());

    let id = {
        let out = bl(repo.path())
            .env("PATH", path_with_mock(bin_dir.path()))
            .args(["create", "show external"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let out = bl(repo.path()).args(["show", &id]).output().unwrap();
    assert!(out.status.success(), "show failed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("remote:"), "missing remote: block in:\n{stdout}");
    assert!(
        stdout.contains(&format!("mock: MOCK-{id}")),
        "missing 'mock: MOCK-{id}' in:\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("https://mock.example/MOCK-{id}")),
        "missing remote_url in:\n{stdout}"
    );
}

#[test]
fn show_renders_external_with_only_remote_key() {
    let repo = new_repo();
    init_in(repo.path());
    let id = {
        let out = bl(repo.path()).args(["create", "key only"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    rewrite_external(
        repo.path(),
        &id,
        serde_json::json!({ "jira": { "remote_key": "PROJ-42" } }),
    );

    let out = bl(repo.path()).args(["show", &id]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("jira: PROJ-42"), "want 'jira: PROJ-42' in:\n{stdout}");
}

#[test]
fn show_renders_external_with_only_remote_url() {
    let repo = new_repo();
    init_in(repo.path());
    let id = {
        let out = bl(repo.path()).args(["create", "url only"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    rewrite_external(
        repo.path(),
        &id,
        serde_json::json!({ "slack": { "remote_url": "https://slack.example/thread/1" } }),
    );

    let out = bl(repo.path()).args(["show", &id]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("slack: https://slack.example/thread/1"),
        "want 'slack: https://...' in:\n{stdout}"
    );
}

#[test]
fn show_skips_external_without_remote_fields() {
    let repo = new_repo();
    init_in(repo.path());
    let id = {
        let out = bl(repo.path())
            .args(["create", "no remote fields"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    rewrite_external(
        repo.path(),
        &id,
        serde_json::json!({ "opaque": { "arbitrary": "payload" } }),
    );

    let out = bl(repo.path()).args(["show", &id]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("remote:"),
        "should not render a remote: block when no recognizable fields; got:\n{stdout}"
    );
}
