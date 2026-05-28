//! Config-file seeding helpers for integration tests: writing a
//! pre-`bl init` `.balls/config.json` and editing the project-owned
//! `.balls/project.json` plugin map (SPEC §7). Split out of
//! `common/mod.rs` to keep it under the line cap; `mod.rs` re-exports
//! both so call sites keep using `common::seed_config` etc.

#![allow(dead_code)]

use std::path::Path;

use super::paths::{config_path, discover_state_repo, project_config_path};
use super::{commit_state_repo, git};

/// Write a minimal valid `.balls/config.json` before `bl init`, with
/// `extra` string-valued keys merged into the base object. Use this
/// instead of hand-rolling the config JSON literal per test — see
/// `common::forge::seed` for the deferred-delivery variant.
pub fn seed_config(repo: &Path, extra: &[(&str, &str)]) {
    let mut cfg = serde_json::json!({
        "version": 1,
        "id_length": 4,
        "stale_threshold_seconds": 60,
        "worktree_dir": ".balls-worktrees",
    });
    let obj = cfg.as_object_mut().unwrap();
    for (key, val) in extra {
        obj.insert((*key).into(), serde_json::Value::String((*val).into()));
    }
    let balls = repo.join(".balls");
    std::fs::create_dir_all(&balls).unwrap();
    std::fs::write(balls.join("config.json"), cfg.to_string()).unwrap();
}

/// Edit the per-repo config in place and commit it to its hosting
/// branch, layout-aware. Legacy: `.balls/config.json` on the code
/// branch (committed in the code repo). XDG: `repo.json` on
/// `balls/tasks` (committed in the tracker checkout). `mutate` runs
/// against the parsed JSON; an empty mutate is a no-op-on-disk
/// rewrite (still committed so tests can assert a fresh tip).
pub fn edit_and_commit_repo_config(
    repo: &Path,
    msg: &str,
    mutate: impl FnOnce(&mut serde_json::Value),
) {
    let cp = config_path(repo);
    let mut cfg: serde_json::Value = std::fs::read_to_string(&cp)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    mutate(&mut cfg);
    std::fs::write(&cp, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    if let Some(sr) = discover_state_repo(repo) {
        if cp.starts_with(&sr) {
            commit_state_repo(repo, msg);
            return;
        }
    }
    git(repo, &["add", ".balls/config.json"]);
    git(repo, &["commit", "-m", msg]);
}

/// Set the `plugins` map in `.balls/project.json` — the project config
/// on the tracker branch (SPEC §7). The path is a symlink into the
/// state checkout, so the write lands there; `commit_state_repo`
/// publishes it. Use after `bl init` has seeded `project.json`.
pub fn set_project_plugins(repo: &Path, plugins: serde_json::Value) {
    let path = project_config_path(repo);
    let mut cfg: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    cfg["plugins"] = plugins;
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
}
