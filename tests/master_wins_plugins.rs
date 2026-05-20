//! bl-a7d9 — `master_url` makes the hub authoritative for plugin
//! config. The project's `.balls/plugins/*` and committed `plugins`
//! map are ignored; effective dispatch reads from the materialized
//! `.balls/state-repo/`.
//!
//! Conformance:
//! - A repo flipping on `master_url` while still carrying project-side
//!   plugin config emits a one-shot "master wins" drift warning.
//! - Standalone repos (no `master_url`) keep reading project-side
//!   plugin config — no warning, no behavior change.
//! - Plugin dispatch resolves per-plugin config files relative to the
//!   hub-side state-repo, not the project root.

mod common;

use common::plugin::*;
use common::*;
use std::fs;

const MASTER_WINS_MARKER: &str = "master wins";

/// Once `master_url` is wired, a leftover project-side plugin entry
/// counts as drift and the operator gets nudged to migrate.
#[test]
fn master_url_warns_when_project_carries_plugin_drift() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    configure_plugin(alice.path());

    bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();

    let out = bl(alice.path()).arg("ready").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(MASTER_WINS_MARKER),
        "expected master-wins drift warning on stderr, got: {stderr}"
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
/// `config_file` relative to the materialized state-repo. The mock
/// plugin logs the `--config` path it receives; we assert it points
/// inside `.balls/state-repo/`, never straight off the project root.
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

    let state_repo_str = state_repo.to_string_lossy();
    let project_plugins_str = alice.path().join(".balls/plugins").to_string_lossy().into_owned();
    for line in log_contents.lines() {
        let Some(cfg) = line.split("config=").nth(1).and_then(|s| s.split_whitespace().next())
        else {
            continue;
        };
        assert!(
            cfg.starts_with(state_repo_str.as_ref()),
            "plugin config path must resolve into the hub-side state-repo, got `{cfg}`"
        );
        assert!(
            !cfg.starts_with(&project_plugins_str),
            "plugin config path must not resolve to the project-side plugins dir, got `{cfg}`"
        );
    }
}
