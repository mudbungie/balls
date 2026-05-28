//! Unit coverage for `Store::load_effective_config` and the
//! integration-branch helpers. Both layouts go through here — XDG
//! resolves the §6.5 merger; Legacy adapts the on-disk `Config`.

use crate::clone_json::CloneJson;
use crate::config::Config;
use crate::config_blocks::{Delivery, DeliveryMode, ReviewConfig};
use crate::git_test_support::{git_run, init_repo};
use crate::layered_fields::{Integrate, IntegrateMode, ReviewBlock};
use crate::repo_json::RepoJson;
use crate::store::{Layout, Store};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn xdg_store(root: &Path, repo: RepoJson, clone: Option<CloneJson>) -> Store {
    let balls = root.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    let repo_path = balls.join("repo.json");
    fs::write(&repo_path, repo.to_json().unwrap() + "\n").unwrap();
    let project_path = balls.join("project.json");
    fs::write(&project_path, "{}\n").unwrap();
    Store {
        root: root.to_path_buf(),
        stealth: false,
        no_git: false,
        layout: Layout::Xdg,
        tasks_dir_path: balls.join("tasks"),
        state_repo_path: root.to_path_buf(),
        state_branch_name: "balls/tasks".into(),
        claims_dir_path: balls.join("claims"),
        lock_dir_path: balls.join("locks"),
        local_plugins_dir_path: balls.join("plugins-auth"),
        worktrees_root_path: root.join("worktrees"),
        config_file_path: repo_path,
        project_config_file_path: project_path,
        clone_json: clone,
    }
}

fn legacy_store(root: &Path, cfg: &Config) -> Store {
    let balls = root.join(".balls");
    fs::create_dir_all(&balls).unwrap();
    let cfg_path = balls.join("config.json");
    cfg.save(&cfg_path).unwrap();
    let project_path = balls.join("project.json");
    fs::write(&project_path, "{}\n").unwrap();
    Store {
        root: root.to_path_buf(),
        stealth: false,
        no_git: false,
        layout: Layout::Legacy,
        tasks_dir_path: balls.join("tasks"),
        state_repo_path: balls.join("state-repo"),
        state_branch_name: "balls/tasks".into(),
        claims_dir_path: balls.join("claims"),
        lock_dir_path: balls.join("locks"),
        local_plugins_dir_path: balls.join("plugins"),
        worktrees_root_path: root.join(".balls-worktrees"),
        config_file_path: cfg_path,
        project_config_file_path: project_path,
        clone_json: None,
    }
}

fn legacy_cfg_with(mutate: impl FnOnce(&mut Config)) -> Config {
    let mut cfg = Config::default();
    mutate(&mut cfg);
    cfg
}

#[test]
fn xdg_carries_integrate_and_gate_command_through_resolver() {
    let td = TempDir::new().unwrap();
    let repo = RepoJson {
        integrate: Some(Integrate { mode: IntegrateMode::ForgePr }),
        review: Some(ReviewBlock { gate_command: Some("make check".into()) }),
        ..RepoJson::default()
    };
    let store = xdg_store(td.path(), repo, None);
    let ec = store.load_effective_config().unwrap();
    assert_eq!(ec.integrate.mode, IntegrateMode::ForgePr);
    assert_eq!(ec.review.gate_command.as_deref(), Some("make check"));
}

#[test]
fn xdg_clone_json_overrides_repo_json() {
    let td = TempDir::new().unwrap();
    let repo = RepoJson {
        integrate: Some(Integrate { mode: IntegrateMode::Direct }),
        ..RepoJson::default()
    };
    let clone = CloneJson {
        integrate: Some(Integrate { mode: IntegrateMode::ForgePr }),
        ..CloneJson::default()
    };
    let store = xdg_store(td.path(), repo, Some(clone));
    assert_eq!(
        store.load_effective_config().unwrap().integrate.mode,
        IntegrateMode::ForgePr,
    );
}

#[test]
fn legacy_local_squash_adapts_to_direct() {
    let td = TempDir::new().unwrap();
    let cfg = legacy_cfg_with(|c| {
        c.delivery = Some(Delivery { mode: DeliveryMode::LocalSquash });
    });
    let store = legacy_store(td.path(), &cfg);
    assert_eq!(
        store.load_effective_config().unwrap().integrate.mode,
        IntegrateMode::Direct,
    );
}

#[test]
fn legacy_deferred_adapts_to_forge_pr() {
    let td = TempDir::new().unwrap();
    let cfg = legacy_cfg_with(|c| {
        c.delivery = Some(Delivery { mode: DeliveryMode::Deferred });
        c.review = Some(ReviewConfig { pre_check: Some("scripts/check".into()) });
        c.require_remote_on_claim = true;
        c.stale_threshold_seconds = 42;
        c.worktree_dir = "custom-wt".into();
    });
    let store = legacy_store(td.path(), &cfg);
    let ec = store.load_effective_config().unwrap();
    assert_eq!(ec.integrate.mode, IntegrateMode::ForgePr);
    assert_eq!(ec.review.gate_command.as_deref(), Some("scripts/check"));
    assert!(ec.require_remote_on_claim);
    assert_eq!(ec.stale_threshold_seconds, 42);
    assert_eq!(ec.worktree_dir.as_deref(), Some("custom-wt"));
}

#[test]
fn explicit_repo_target_branch_is_none_under_xdg() {
    let td = TempDir::new().unwrap();
    let store = xdg_store(td.path(), RepoJson::default(), None);
    assert_eq!(store.explicit_repo_target_branch().unwrap(), None);
}

#[test]
fn explicit_repo_target_branch_carries_legacy_field() {
    let td = TempDir::new().unwrap();
    let cfg = legacy_cfg_with(|c| c.target_branch = Some("develop".into()));
    let store = legacy_store(td.path(), &cfg);
    assert_eq!(
        store.explicit_repo_target_branch().unwrap().as_deref(),
        Some("develop"),
    );
}

#[test]
fn integration_branch_for_returns_task_target_when_set() {
    let td = TempDir::new().unwrap();
    // No git repo needed: per-task target short-circuits before the
    // HEAD lookup.
    let store = xdg_store(td.path(), RepoJson::default(), None);
    assert_eq!(
        store.integration_branch_for(Some("hotfix")).unwrap(),
        "hotfix",
    );
}

#[test]
fn integration_branch_falls_back_to_legacy_repo_target_branch() {
    let td = TempDir::new().unwrap();
    let cfg = legacy_cfg_with(|c| c.target_branch = Some("develop".into()));
    let store = legacy_store(td.path(), &cfg);
    assert_eq!(store.integration_branch().unwrap(), "develop");
}

#[test]
fn integration_branch_falls_through_to_head_when_no_overrides() {
    let td = TempDir::new().unwrap();
    init_repo(td.path());
    git_run(td.path(), &["checkout", "-b", "feature/foo"]);
    let store = xdg_store(td.path(), RepoJson::default(), None);
    assert_eq!(store.integration_branch().unwrap(), "feature/foo");
}
