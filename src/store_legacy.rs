//! Legacy in-repo layout helpers for `Store::discover`. Phase 1A
//! splits the legacy branch out of `store.rs` so the main module
//! stays under the 300-line cap and the dual-read paths are visually
//! separated. The behavior here is unchanged from pre-bl-203e —
//! `.balls/config.json` on main, `.balls/state-repo/` runtime,
//! `.balls-worktrees/` colocated in tree.
//!
//! `bl migrate` (Phase 2, bl-717e) is the off-ramp from this layout.
//! Phase 1 reads it; Phase 2 moves it.

use crate::config::Config;
use crate::error::{BallError, Result};
use crate::git;
use crate::state_repo::STATE_REPO_REL;
use crate::store::{Layout, Store};
use crate::store_paths::{find_balls_root, stealth_tasks_override};
use crate::tracker_address;
use std::path::{Path, PathBuf};

/// Resolve a legacy-mode Store rooted at `main_root`. SPEC §12 row 2:
/// emit the one-line "legacy layout in use" warning, then continue.
/// `bl migrate` (Phase 2) is the off-ramp; reads stay correct here.
pub(crate) fn discover(main_root: &Path) -> Result<Store> {
    emit_legacy_warning(main_root);
    if let Some(tasks_dir_path) = stealth_tasks_override(main_root) {
        return Ok(legacy_stealth(main_root.to_path_buf(), tasks_dir_path));
    }
    let state_repo_path = ensure_state_repo(main_root)?;
    let tasks_dir_path = state_repo_path.join(".balls/tasks");
    if !tasks_dir_path.exists() {
        return Err(BallError::state_worktree_missing(main_root, &tasks_dir_path));
    }
    let state_branch_name = resolve_state_branch(&state_repo_path);
    Ok(legacy_with(
        main_root.to_path_buf(),
        tasks_dir_path,
        state_repo_path,
        state_branch_name,
        false,
    ))
}

/// No-git discovery — `bl` invoked outside any git repo. The only
/// state shape that can resolve here is a stealth clone with an
/// already-recorded `tasks_dir`; non-stealth needs git for origin
/// resolution. XDG-mode no-git is Phase 1B's stealth clone.json
/// flow; Phase 1A only finds the legacy `.balls/local/tasks_dir`
/// marker (`stealth_tasks_override`).
pub(crate) fn discover_no_git(from: &Path) -> Result<Store> {
    let root = find_balls_root(from)?;
    let tasks_dir = stealth_tasks_override(&root);
    if let Some(p) = tasks_dir.as_ref().filter(|p| p.exists()) {
        let mut store = legacy_stealth(root, p.clone());
        store.no_git = true;
        return Ok(store);
    }
    let had = tasks_dir.is_some();
    let shown = tasks_dir.unwrap_or_else(|| root.join(".balls/tasks"));
    Err(BallError::no_git_store_unusable(&root, &shown, had))
}

/// Build a legacy-mode Store from explicit path fields. The three
/// sites that produce a Store rooted in the in-repo layout —
/// `discover`, `discover_no_git`, and `Store::init` — all funnel
/// through here so the legacy path bundle stays in one place.
pub(crate) fn legacy_with(
    root: PathBuf,
    tasks_dir_path: PathBuf,
    state_repo_path: PathBuf,
    state_branch_name: String,
    stealth: bool,
) -> Store {
    let local = root.join(".balls/local");
    let balls = root.join(".balls");
    Store {
        claims_dir_path: local.join("claims"),
        lock_dir_path: local.join("lock"),
        local_plugins_dir_path: local.join("plugins"),
        worktrees_root_path: root.join(".balls-worktrees"),
        config_file_path: balls.join("config.json"),
        project_config_file_path: balls.join("project.json"),
        root,
        stealth,
        no_git: false,
        layout: Layout::Legacy,
        tasks_dir_path,
        state_repo_path,
        state_branch_name,
        // Legacy layout predates clone.json (SPEC §6.4 is XDG-only).
        clone_json: None,
    }
}

fn legacy_stealth(root: PathBuf, tasks_dir: PathBuf) -> Store {
    let state_repo = root.join(STATE_REPO_REL);
    legacy_with(
        root,
        tasks_dir,
        state_repo,
        tracker_address::DEFAULT_BRANCH.to_string(),
        true,
    )
}

/// Materialize `.balls/state-repo` if absent, returning its path.
/// A warm checkout is returned with best-effort project-config
/// migration and branch alignment — no network round-trip.
/// Failures in the warm fast path fall through silently so a
/// broken state-repo or corrupt config stays discoverable for
/// `bl doctor` to surface the real problem.
fn ensure_state_repo(root: &Path) -> Result<PathBuf> {
    let dir = root.join(STATE_REPO_REL);
    if dir.join(".git").exists() {
        crate::state_repo::ensure_project_config(root, &dir)?;
        if let Ok(cfg) = Config::load(&root.join(".balls/config.json")) {
            let addr = tracker_address::resolve(root, &cfg);
            let _ = crate::state_repo::align_warm_branch(&dir, &addr.branch);
        }
        crate::legacy_plugin_migrate::run(root)?; // bl-de57 warm-path self-heal
        return Ok(dir);
    }
    let cfg = Config::load(&root.join(".balls/config.json"))?;
    let addr = tracker_address::resolve(root, &cfg);
    crate::state_repo::ensure(root, &addr)
}

/// Resolve the state branch from `.balls/state-repo`'s HEAD. Falls
/// back to the SPEC default when the checkout has none yet.
fn resolve_state_branch(state_repo: &Path) -> String {
    git::git_current_branch(state_repo)
        .unwrap_or_else(|_| tracker_address::DEFAULT_BRANCH.to_string())
}

/// SPEC §12 row 2: one-line nudge to migrate. Phase 3 (bl-05e5) makes
/// the line *specific* — it names the legacy marker found (e.g.
/// `.balls/config.json`) and suggests `bl prime --migrate` so the
/// user has a one-step off-ramp from the surface they already use.
fn emit_legacy_warning(root: &Path) {
    let markers = crate::legacy_layout::detect(root);
    if !markers.is_empty() {
        eprintln!("{}", crate::legacy_layout::warning_line(&markers));
    }
}

#[cfg(test)]
#[path = "store_legacy_tests.rs"]
mod tests;
