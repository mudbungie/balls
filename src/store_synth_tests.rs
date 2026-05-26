//! Unit coverage for `Store::synthesize_xdg_config`: builds a minimal
//! XDG-shaped `Store` against a tempdir with a hand-rolled `repo.json`
//! and checks the synthesized `Config` carries `integrate.mode`
//! through (both `Direct` and `ForgePr`) and `review.gate_command`
//! through to `pre_check`.

use super::*;
use crate::config_blocks::DeliveryMode;
use crate::layered_fields::{Integrate, IntegrateMode, ReviewBlock};
use crate::repo_json::RepoJson;
use std::fs;
use tempfile::TempDir;

fn xdg_store(root: &Path, repo: RepoJson) -> Store {
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
        clone_json: None,
    }
}

#[test]
fn synthesize_maps_integrate_direct_to_local_squash() {
    let td = TempDir::new().unwrap();
    let repo = RepoJson {
        integrate: Some(Integrate { mode: IntegrateMode::Direct }),
        ..RepoJson::default()
    };
    let store = xdg_store(td.path(), repo);
    let cfg = store.load_config().unwrap();
    assert_eq!(cfg.delivery.unwrap().mode, DeliveryMode::LocalSquash);
}

#[test]
fn synthesize_maps_integrate_forge_pr_to_deferred() {
    let td = TempDir::new().unwrap();
    let repo = RepoJson {
        integrate: Some(Integrate { mode: IntegrateMode::ForgePr }),
        ..RepoJson::default()
    };
    let store = xdg_store(td.path(), repo);
    let cfg = store.load_config().unwrap();
    assert_eq!(cfg.delivery.unwrap().mode, DeliveryMode::Deferred);
}

#[test]
fn synthesize_carries_review_gate_command_to_pre_check() {
    let td = TempDir::new().unwrap();
    let repo = RepoJson {
        review: Some(ReviewBlock {
            gate_command: Some("make check".into()),
        }),
        ..RepoJson::default()
    };
    let store = xdg_store(td.path(), repo);
    let cfg = store.load_config().unwrap();
    assert_eq!(cfg.review.unwrap().pre_check.as_deref(), Some("make check"));
}
