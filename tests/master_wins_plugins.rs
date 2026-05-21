//! bl-a7d9 + bl-1098 + bl-82a4 — `master_url` makes the hub
//! authoritative for plugin config. Federated mode is a filesystem
//! fact: `.balls/plugins/` is a symlink to `state-repo/.balls/plugins`
//! and `.balls/config.json` a symlink to the hub's canonical. No
//! runtime branching on `master_url`.
//!
//! Conformance:
//! - `bl remaster <url> --commit` *migrates* a leftover project-side
//!   plugin entry up to the hub (bl-82a4 — no refuse, no silent
//!   drift): the stdout names the promoted plugin and the on-disk
//!   shape leaves `.balls/plugins/` a symlink.
//! - Standalone repos (no `master_url`) keep `.balls/plugins/` a real
//!   directory — no behavior change.
//! - Plugin dispatch resolves per-plugin config files through the
//!   project root, which the symlink redirects into the hub view.

mod common;

use common::plugin::*;
use common::*;
use std::fs;

/// A leftover project-side plugin entry is *migrated* up to the hub
/// at `bl remaster <url> --commit` time (bl-82a4). The migration's
/// stdout names the promoted plugin, and the on-disk shape leaves
/// `.balls/plugins/` a symlink — nothing left to drift against.
#[test]
fn master_url_promotes_project_plugins_during_remaster() {
    let hub = new_bare_remote();
    let alice = new_repo();
    init_in(alice.path());
    configure_plugin(alice.path());

    let out = bl(alice.path())
        .arg("remaster")
        .arg(hub.path().to_string_lossy().as_ref())
        .arg("--commit")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(
        stdout.contains("promoted plugins to hub: mock"),
        "expected promotion summary on stdout, got: {stdout}"
    );
    assert!(
        alice.path().join(".balls/plugins").is_symlink(),
        "after federate, project-side .balls/plugins must be a symlink"
    );
}

/// Standalone repos keep `.balls/plugins/` as a real directory.
#[test]
fn standalone_repo_keeps_real_plugins_directory() {
    let repo = new_repo();
    init_in(repo.path());
    configure_plugin(repo.path());

    let plugins_dir = repo.path().join(".balls/plugins");
    assert!(plugins_dir.is_dir(), "standalone .balls/plugins is a real dir");
    assert!(
        !plugins_dir.is_symlink(),
        "standalone must not symlink .balls/plugins"
    );
}

/// With `master_url`, plugin dispatch resolves the per-plugin
/// `config_file` relative to the project root — but `.balls/plugins`
/// is a symlink to the state-repo, so the path canonicalizes into the
/// hub view. The mock plugin logs the `--config` path it receives; we
/// canonicalize and assert it resolves under `.balls/state-repo/`.
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

    // Hub-side plugin config lives on the materialized state-repo's
    // `balls/tasks` checkout. Shortcut the maintainer workflow by
    // writing and committing directly to the local state-repo.
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
            "plugin config path must canonicalize into the hub-side state-repo \
             via the .balls/plugins symlink, got `{cfg}` (canonical `{}`)",
            canon.display()
        );
    }
}
