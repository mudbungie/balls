//! bl-a7d9 + bl-1098 — `master_url` makes the hub authoritative for
//! plugin config. The transition to federated is now mediated by a
//! `.balls/plugins -> state-repo/.balls/plugins` symlink (option a):
//! reads always go through the project root, and the symlink
//! redirects to the hub view. No runtime branching on `master_url`.
//!
//! Conformance:
//! - A repo flipping on `master_url` while still carrying project-side
//!   plugin config files is refused at `bl remaster --commit` time
//!   with a migration message — the silent-drift window is gone.
//! - Standalone repos (no `master_url`) keep reading project-side
//!   plugin config — no warning, no behavior change.
//! - Plugin dispatch resolves per-plugin config files through the
//!   project-root path, which the symlink redirects to the hub view
//!   so the file content comes from the state-repo.

mod common;

use common::plugin::*;
use common::*;
use std::fs;

const MASTER_WINS_MARKER: &str = "master wins";

/// Flipping on `master_url` while project-side plugin files are still
/// present is refused at remaster time with a migration message — the
/// hub-wins rule would otherwise silently shadow those files (bl-1098).
#[test]
fn remaster_commit_refuses_when_project_carries_plugin_drift() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    configure_plugin(alice.path());

    let out = bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected remaster --commit to refuse");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("hub is authoritative") || stderr.contains("bl-a7d9"),
        "refusal must reference the master-wins rule: {stderr}"
    );
}

/// Standalone repos must NOT emit the master-wins warning —
/// byte-identical to pre-bl-a7d9 behavior.
#[test]
fn standalone_repo_emits_no_master_wins_warning() {
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());

    let out = bl(repo.path()).arg("ready").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains(MASTER_WINS_MARKER),
        "standalone repo must not see master-wins warning: {stderr}"
    );
}

/// With `master_url`, plugin dispatch resolves the per-plugin
/// `config_file` relative to the project root — but `.balls/plugins`
/// is now a symlink to the state-repo (bl-1098), so the path
/// canonicalizes into the hub view. The mock plugin logs the
/// `--config` path it receives; we canonicalize and assert it resolves
/// under `.balls/state-repo/`.
#[test]
fn master_url_dispatches_with_hub_side_plugin_config_path() {
    let (bin_dir, log) = install_mock_plugin();
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();

    // Hub-side plugin config: live on the materialized state-repo's
    // `balls/tasks` checkout. The maintainer's normal workflow lands
    // these files via the same path; we shortcut that here by writing
    // and committing directly to the local state-repo.
    let state_repo = alice.path().join(".balls/state-repo");
    let state_balls = state_repo.join(".balls");
    fs::create_dir_all(state_balls.join("plugins")).unwrap();
    fs::write(
        state_balls.join("plugins/mock.json"),
        r#"{"url":"https://hub.example"}"#,
    )
    .unwrap();
    fs::write(
        state_balls.join("config.json"),
        r#"{"version":1,"id_length":4,"stale_threshold_seconds":60,
            "worktree_dir":".balls-worktrees",
            "plugins":{"mock":{"enabled":true,"sync_on_change":false,
                               "config_file":".balls/plugins/mock.json"}}}"#,
    )
    .unwrap();
    git(&state_repo, &["add", ".balls/config.json", ".balls/plugins/mock.json"]);
    git(&state_repo, &["commit", "-m", "hub: publish mock plugin", "--no-verify"]);
    create_mock_auth(alice.path());

    bl(alice.path())
        .env("PATH", path_with_mock(bin_dir.path()))
        .args(["create", "exercise hub plugin"])
        .assert()
        .success();

    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    assert!(
        log_contents.contains("config="),
        "mock plugin must have been invoked with a --config path: {log_contents}"
    );

    let state_repo_canon = fs::canonicalize(&state_repo).unwrap();
    for line in log_contents.lines() {
        let Some(cfg) = line.split("config=").nth(1).and_then(|s| s.split_whitespace().next())
        else {
            continue;
        };
        let canon = fs::canonicalize(cfg).expect("plugin config path must exist");
        assert!(
            canon.starts_with(&state_repo_canon),
            "plugin config path must resolve into the hub-side state-repo via the \
             .balls/plugins symlink, got `{cfg}` (canonical `{}`)",
            canon.display()
        );
    }
}
