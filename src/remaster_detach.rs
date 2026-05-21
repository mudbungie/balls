//! `bl remaster --detach` — sever a repo's shared task history and
//! return it to a standalone local store. Split from `remaster.rs`
//! (the reconcile core) to keep both files under the line cap.
//!
//! Detach has a warm and a cold path. The warm `detach` runs once the
//! state checkout has materialized: re-root the `balls/tasks` orphan
//! so it no longer descends from any hub. The cold `try_cold_detach`
//! (bl-dcd3) breaks the deadlock where `master_url` is set but the
//! state-repo never materialized — `Store::discover` re-hits the same
//! hard-fail, so detach must run without it.
//!
//! Both paths must leave the post-detach layout as the one a fresh
//! `Store::discover` resolves. With `master_url` cleared that is the
//! legacy `.balls/worktree`; under `master_url` the re-rooted orphan
//! lives in the balls-owned `.balls/state-repo` clone, so the warm
//! path transplants it onto `.balls/worktree` (bl-f440).

use crate::error::Result;
use crate::store::Store;
use crate::store_init::STATE_WORKTREE_REL;
use crate::{git, git_state};
use std::fs;
use std::path::{Path, PathBuf};

const STATE_BRANCH: &str = "balls/tasks";

/// Sever shared history: re-root `balls/tasks` as a fresh local
/// orphan carrying its current tasks. The config half (clearing
/// `master_url`/`state_remote`) is the caller's job.
///
/// bl-1098: also reverse the `.balls/plugins -> state-repo/.balls/
/// plugins` symlink — going standalone means the project owns plugin
/// config again, so we materialize a real `.balls/plugins/` carrying
/// the hub's files at the moment of detach. Skipped for the legacy
/// `state_remote` path (no symlink ever existed there).
///
/// bl-f440: under `master_url` the state checkout is the balls-owned
/// `.balls/state-repo` clone, but clearing `master_url` makes every
/// later `discover` resolve `.balls/worktree`. Once the orphan is
/// re-rooted, transplant it onto the project git's `.balls/worktree`
/// — otherwise `discover` reads a stale `.balls/worktree` (or
/// hard-fails when none exists) and the detached task set is lost.
pub fn detach(store: &Store) -> Result<()> {
    let sd = store.state_worktree_dir();
    let plugins_link = store.root.join(".balls/plugins");
    if plugins_link.is_symlink() {
        crate::state_repo::restore_plugins_dir(&store.root, &sd.join(".balls/plugins"))?;
    }
    git_state::reroot_orphan(&sd, STATE_BRANCH, "balls: remaster --detach (standalone)")?;
    let worktree = store.root.join(STATE_WORKTREE_REL);
    if sd != worktree {
        adopt_state_into_worktree(&store.root, &sd, &worktree)?;
    }
    Ok(())
}

/// Transplant a re-rooted `balls/tasks` out of the balls-owned
/// `.balls/state-repo` clone onto the project git's `.balls/worktree`
/// — the layout a post-detach `discover` resolves once `master_url`
/// is cleared. Mirrors `try_cold_detach`'s `setup_state_branch`, but
/// carries the federated task history across; the cold path has none.
fn adopt_state_into_worktree(root: &Path, state_repo: &Path, worktree: &Path) -> Result<()> {
    // A pre-flip standalone era can leave a stale `.balls/worktree`
    // checked out on an unrelated `balls/tasks`; tear it down so the
    // branch is free to be force-updated and re-checked-out.
    if worktree.exists() {
        let _ = git::git_worktree_remove(root, worktree, true);
        let _ = fs::remove_dir_all(worktree);
    }
    let _ = git_state::worktree_prune(root);
    git_state::fetch_into_branch(root, state_repo, STATE_BRANCH)?;
    git_state::worktree_add_existing(root, worktree, STATE_BRANCH)?;
    crate::store_init::ensure_tasks_symlink(root, "worktree/.balls/tasks")
}

/// Offline-friendly detach (bl-dcd3) for when `master_url` is set but
/// the state-repo hasn't yet materialized — typically because the hub
/// is unreachable and `state_repo::ensure` hard-failed first-time
/// setup. The warm `detach` path can't run there: `Store::discover`
/// re-hits the same hard-fail, so this is the deadlock breaker.
///
/// Returns `Ok(true)` when the cold path applied (caller should
/// stop), `Ok(false)` when it doesn't apply and the caller should
/// fall through to the warm `detach` flow. Strictly local — clears
/// `master_url`/`state_remote` in committed config and re-initializes
/// the legacy `.balls/worktree` layout on the project's own git, no
/// network round-trip.
pub fn try_cold_detach(from: &Path) -> Result<bool> {
    let repo_root = project_root(from)?;
    let config_path = repo_root.join(".balls/config.json");
    let mut cfg = crate::config::Config::load(&config_path)?;
    if cfg.master_url().is_none() {
        return Ok(false);
    }
    if repo_root
        .join(crate::state_repo::STATE_REPO_REL)
        .join(".git")
        .exists()
    {
        // Warm cache available: the regular `discover` + `detach`
        // path can re-root the orphan and preserve task data.
        return Ok(false);
    }
    cfg.master_url = None;
    cfg.state_remote = None;
    cfg.save(&config_path)?;
    git::git_ensure_user(&repo_root)?;
    crate::store_init::setup_state_branch(&repo_root, "origin", false)?;
    Ok(true)
}

fn project_root(from: &Path) -> Result<PathBuf> {
    crate::store_paths::require_git_repo(from)?;
    let common = git::git_common_dir(from)?;
    crate::store_paths::find_main_root(&common)
}
